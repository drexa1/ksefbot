use super::Tui;
use super::widgets::{confirm, message, select, select_index, text_input};
use crate::api::invoices::create::{SalesInvoice, UploadInvoiceResult, create_invoice, download_receipt, load_invoice_parties, preview_sales_invoice, submit_invoice, upload_invoice};
use crate::api::users::AppUser;
use crate::{cf_client_id, cf_client_secret, cf_worker_url};
use anyhow::Result;
use chrono::NaiveDate;
use std::fs::{create_dir_all, read_dir, read_to_string, write};
use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvoiceType { Sales, Purchases }

impl std::fmt::Display for InvoiceType {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self { InvoiceType::Sales => "sales", InvoiceType::Purchases => "purchases" })
    }
}

impl InvoiceType {
    fn counterparty_key(&self) -> &'static str {
        match self { InvoiceType::Sales => "Buyer", InvoiceType::Purchases => "Seller" }
    }

    fn counterparty_label(&self) -> &'static str {
        match self { InvoiceType::Sales => "Customer", InvoiceType::Purchases => "Seller" }
    }
}

pub async fn prompt_create_invoice(terminal: &mut Tui, app_user: &AppUser) -> Result<Vec<String>> {
    let mut log = Vec::new();
    let generated_files = generated_invoice_files()?;
    let import_invoice = if generated_files.is_empty() {
        false
    } else {
        match select_index(terminal, "Create sales invoice", &[
            "Create a new invoice".to_string(),
            "Import a generated XML".to_string()
        ], &mut 0)? {
            Some(index) => index == 1,
            None => return Ok(Vec::new()),
        }
    };
    let (mut new_invoice, imported_path) = if import_invoice {
        let Some((invoice, path)) = import_generated_invoice(terminal, &generated_files)? else {
            return Ok(vec!["Invoice creation cancelled.".to_string()]);
        };
        (invoice, Some(path))
    } else {
        let invoice_parties = load_invoice_parties(app_user).await?;
        let customer = if invoice_parties.customers.len() == 1 {
            &invoice_parties.customers[0]
        } else {
            let choices: Vec<String> = invoice_parties.customers.iter()
                .map(|customer| format!("{} (NIP: {})", customer.name, customer.nip.as_deref().unwrap_or("-")))
                .collect();
            let Some(index) = select_index(terminal, "Select customer", &choices, &mut 0)? else {
                return Ok(vec!["Invoice creation cancelled.".to_string()]);
            };
            &invoice_parties.customers[index]
        };
        let hours_worked = loop {
            let Some(value) = text_input(terminal, "Hours worked", "")? else {
                return Ok(vec!["Invoice creation cancelled.".to_string()]);
            };
            match value.trim().parse::<u32>() {
                Ok(hours) if hours > 0 => break hours,
                _ => message(terminal, "Validation", &["The number of hours must be greater than zero.".to_string()])?,
            }
        };
        let preview = preview_sales_invoice(app_user, hours_worked)?;
        message(terminal, "Invoice preview", &[
            format!("Hours worked: {hours_worked}"),
            format!("Hourly rate: {:.2} PLN/h", preview.hourly_rate),
            format!("Total net: {:.2} PLN", preview.total_net),
            format!("Total VAT: {:.2} PLN", preview.total_vat),
            format!("Total gross: {:.2} PLN", preview.total_gross),
        ])?;
        let new_invoice = create_invoice(app_user, &invoice_parties.user_contractor, customer, hours_worked).await?;
        if confirm(terminal, "Save this invoice in your online vault?", true)? {
            let notes = "Invoice generated from CLI client";
            match upload_invoice(app_user, &new_invoice, notes).await? {
                UploadInvoiceResult::Uploaded(invoice_id) => log.push(format!("Invoice '{invoice_id}' uploaded for user '{}'", app_user.id)),
                UploadInvoiceResult::AlreadyExists(invoice_message) => log.push(invoice_message),
            }
        }
        if confirm(terminal, "Save invoice .xml to application folder?", true)? {
            let home = std::env::var_os("USERPROFILE").unwrap();
            let app_folder = PathBuf::from(home).join(".ksefbot").join("generated");
            create_dir_all(&app_folder)?;
            let file_stem = format!("{}-{}", new_invoice.month_name, new_invoice.year);
            let original_path = app_folder.join(format!("{file_stem}.xml"));
            let path = if original_path.exists() {
                match select(terminal, "An invoice file for this month already exists. What shall we do?", &[
                    "Overwrite existing file".to_string(),
                    "Save a new file".to_string()
                ])?.as_deref() {
                    Some("Overwrite existing file") => Some(original_path),
                    Some("Save a new file") => {
                        let mut suffix = 2;
                        Some(loop {
                            let candidate = app_folder.join(format!("{file_stem}-{suffix}.xml"));
                            if !candidate.exists() { break candidate; }
                            suffix += 1;
                        })
                    }
                    _ => None,
                }
            } else {
                Some(original_path)
            };
            if let Some(path) = path {
                write(&path, &new_invoice.xml)?;
                log.push(format!("Invoice .xml saved to {}", path.display()));
            }
        }
        (new_invoice, None)
    };
    if confirm(terminal, "Submit this invoice to KSeF?", false)? {
        let submission = submit_invoice(app_user, &new_invoice).await?;
        new_invoice.submission = Some(submission);
        if let Some(path) = imported_path {
            let submitted_path = move_to_submitted(&path)?;
            log.push(format!("Invoice XML moved to {}", submitted_path.display()));
        }
        log.push("Invoice submitted to KSeF.".to_string());
        log.push(format!("Invoice number: {}", new_invoice.invoice_number));
        let submission = new_invoice.submission.as_ref().unwrap();
        log.push(format!("Invoice reference: {}", submission.invoice_reference_number));
        log.push(format!("KSeF session: {}", submission.session_reference_number));
        if confirm(terminal, "Download the KSeF receipt?", true)? {
            let save_path = download_receipt(app_user, &new_invoice).await?;
            log.push(format!("Submission receipt saved to {}", save_path.display()));
        }
    } else {
        log.push("Invoice created but not submitted.".to_string());
    }
    Ok(log)
}

fn generated_invoice_files() -> Result<Vec<PathBuf>> {
    let home = std::env::var_os("USERPROFILE").unwrap();
    let generated_folder = PathBuf::from(home).join(".ksefbot").join("generated");
    create_dir_all(&generated_folder)?;
    let mut files: Vec<_> = read_dir(&generated_folder)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<io::Result<Vec<_>>>()?;
    files.retain(|path| path.is_file() && path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("xml")));
    files.sort();
    Ok(files)
}

fn import_generated_invoice(terminal: &mut Tui, files: &[PathBuf]) -> Result<Option<(SalesInvoice, PathBuf)>> {
    let mut choices: Vec<String> = files.iter().map(|path| path.file_name().unwrap().to_string_lossy().into_owned()).collect();
    choices.push("Back".to_string());
    let Some(index) = select_index(terminal, "Select a generated invoice XML", &choices, &mut 0)? else { return Ok(None); };
    if index == files.len() { return Ok(None); }
    let xml = read_to_string(&files[index])?;
    Ok(Some((SalesInvoice::from_xml(xml)?, files[index].clone())))
}

fn move_to_submitted(source: &Path) -> Result<PathBuf> {
    let home = std::env::var_os("USERPROFILE").unwrap();
    let submitted_folder = PathBuf::from(home).join(".ksefbot").join("submitted");
    create_dir_all(&submitted_folder)?;
    let file_name = source.file_name().unwrap();
    let target = submitted_folder.join(file_name);
    let target = if target.exists() {
        let stem = source.file_stem().unwrap().to_string_lossy();
        let extension = source.extension().unwrap().to_string_lossy();
        let mut suffix = 2;
        loop {
            let candidate = submitted_folder.join(format!("{stem}-{suffix}.{extension}"));
            if !candidate.exists() { break candidate; }
            suffix += 1;
        }
    } else {
        target
    };
    std::fs::rename(source, &target)?;
    Ok(target)
}

pub fn invoice_preview(invoice: &serde_json::Value, invoice_type: &InvoiceType) -> Vec<String> {
    let body = &invoice["InvoiceBody"];
    let counterparty = &invoice[invoice_type.counterparty_key()]["IdentificationData"];
    let currency = body["CurrencyCode"].as_str().unwrap_or("PLN");
    let mut lines = vec![
        body["InvoiceNumber"].as_str().unwrap_or("-").to_string(),
        String::new(),
        format!("{}: {} - {}", invoice_type.counterparty_label(), counterparty["NIP"].as_str().unwrap_or("-"), counterparty["Name"].as_str().unwrap_or("-")),
        format!("Issue date: {}", body["IssueDate"].as_str().unwrap_or("-")),
        format!("Service date: {}", body["ServiceDate"].as_str().unwrap_or("-")),
        String::new(),
        format!("Net:   {:.2} {currency}", body["TotalNetAmount"].as_f64().unwrap_or_default()),
        format!("VAT:   {:.2} {currency}", body["TotalVatAmount"].as_f64().unwrap_or_default()),
        format!("Gross: {:.2} {currency}", body["TotalGrossAmount"].as_f64().unwrap_or_default()),
        String::new(),
        "Items".to_string(),
    ];
    lines.extend(body["InvoiceLines"].as_array().into_iter().flatten()
        .filter_map(|line| line["ItemDescription"].as_str()).map(str::to_string));
    lines
}

pub async fn list_invoices(app_user: &AppUser, endpoint: &InvoiceType, from: &str, to: &str) -> Result<Vec<serde_json::Value>> {
    let response = crate::api::client::http_client()
        .get(format!("{}/ksef/{endpoint}", cf_worker_url!()))
        .query(&[("from", from), ("to", to)])
        .header("CF-Access-Client-Id", cf_client_id!())
        .header("CF-Access-Client-Secret", cf_client_secret!())
        .header("X-API-Key", app_user.api_key.as_deref().ok_or_else(|| anyhow::anyhow!("User has no API key configured"))?)
        .header("X-User-Id", &app_user.id)
        .header("Accept", "application/json")
        .send()
        .await?;
    let status = response.status();
    let body = response.text().await?;
    decode_invoice_list(status, &body)
}

fn decode_invoice_list(status: reqwest::StatusCode, body: &str) -> Result<Vec<serde_json::Value>> {
    if !status.is_success() {
        let detail = serde_json::from_str::<serde_json::Value>(body).ok()
            .and_then(|json| json["error"].as_str().map(str::to_string))
            .unwrap_or_else(|| body.chars().take(300).collect());
        anyhow::bail!("Invoice request failed (HTTP {status}): {detail}");
    }
    let mut json: serde_json::Value = serde_json::from_str(body)?;
    if json["success"].as_bool() != Some(true) {
        if json["success"].as_bool() == Some(false)
            && json["result"].as_array().is_some_and(Vec::is_empty)
            && json["error"].as_str() == Some("No invoices found for the specified date range.") {
            return Ok(Vec::new());
        }
        anyhow::bail!("Could not load invoices: {}", json["error"].as_str().unwrap_or("unknown error"));
    }

    match json["result"].take() {
        serde_json::Value::Array(array) => Ok(array),
        _ => anyhow::bail!("Invoice response did not contain a list"),
    }
}

pub async fn download_invoice_xml(app_user: &AppUser, invoice_type: &InvoiceType, invoice: &serde_json::Value, invoice_number: &str, from: &str, to: &str) -> Result<PathBuf> {
    let mut url = reqwest::Url::parse(&format!("{}/ksef/{invoice_type}", cf_worker_url!()))?;
    url.query_pairs_mut().append_pair("invoiceNumber", invoice_number).append_pair("from", from).append_pair("to", to);
    let response = crate::api::client::http_client()
        .get(url)
        .header("CF-Access-Client-Id", cf_client_id!())
        .header("CF-Access-Client-Secret", cf_client_secret!())
        .header("X-API-Key", app_user.api_key.as_deref().ok_or_else(|| anyhow::anyhow!("User has no API key configured"))?)
        .header("X-User-Id", &app_user.id)
        .header("Accept", "application/xml")
        .send()
        .await?
        .error_for_status()?;
    let content_type = response.headers().get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("missing Content-Type")
        .to_string();
    let xml = response.text().await?;
    if !content_type.to_ascii_lowercase().contains("xml") || !xml.trim_start().starts_with("<?xml") {
        anyhow::bail!("Expected invoice XML, but the server returned {content_type}.");
    }
    let issue_date = invoice["InvoiceBody"]["IssueDate"].as_str()
        .and_then(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
        .ok_or_else(|| anyhow::anyhow!("Invoice is missing a valid issue date"))?;
    let safe_number: String = invoice_number.chars().map(|character| {
        if character.is_ascii_alphanumeric() || character == '-' || character == '_' { character } else { '_' }
    }).collect();
    let home = std::env::var_os("USERPROFILE").unwrap();
    let download_folder = PathBuf::from(home).join(".ksefbot").join("downloads").join(invoice_type.to_string())
        .join(issue_date.format("%Y").to_string()).join(issue_date.format("%B").to_string());
    std::fs::create_dir_all(&download_folder)?;
    let path = download_folder.join(format!("{safe_number}.xml"));
    std::fs::write(&path, xml)?;
    Ok(path)
}

#[cfg(test)]
mod response_tests {
    use super::*;
    use reqwest::StatusCode;

    #[test]
    fn backend_empty_month_response_is_not_a_failed_request() {
        assert!(decode_invoice_list(StatusCode::OK,
            r#"{"success":false,"result":[],"error":"No invoices found for the specified date range."}"#).unwrap().is_empty());
        assert!(decode_invoice_list(StatusCode::OK, r#"{"success":true,"result":[]}"#).unwrap().is_empty());
        assert_eq!(decode_invoice_list(StatusCode::OK,
            r#"{"success":true,"result":[{"InvoiceBody":{"InvoiceNumber":"FV/1"}}]}"#).unwrap().len(), 1);
    }

    #[test]
    fn backend_errors_keep_their_message_and_are_never_empty_successes() {
        let body = r#"{"success":false,"error":"The maximum date range supported by KSeF is 3 calendar months."}"#;
        let error = decode_invoice_list(StatusCode::BAD_REQUEST, body).unwrap_err().to_string();
        assert!(error.contains("400") && error.contains("maximum date range"));
        assert!(decode_invoice_list(StatusCode::OK, r#"{"success":false,"result":[],"error":"Access denied"}"#).is_err());
        assert!(decode_invoice_list(StatusCode::UNAUTHORIZED,
            r#"{"success":false,"result":[],"error":"No invoices found for the specified date range."}"#).is_err());
        assert!(decode_invoice_list(StatusCode::OK, r#"{"success":true,"result":null}"#).is_err());
        assert!(decode_invoice_list(StatusCode::OK, "invalid json").is_err());
    }
}
