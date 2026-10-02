use crate::api::users::AppUser;
use crate::{cf_client_id, cf_client_secret, cf_worker_url};
use chrono::NaiveDate;
use crossterm::style::Stylize;
use inquire::Select;
use std::path::PathBuf;
use strum::Display;

#[path = "create.rs"]
pub mod create;

#[derive(Clone, Display)]
pub enum InvoiceType {
    #[strum(to_string = "sales")]
    Sales,
    #[strum(to_string = "purchases")]
    Purchases
}

impl InvoiceType {
    fn emoji(&self) -> &'static str {
        match self {
            InvoiceType::Sales => "💵",
            InvoiceType::Purchases => "🛒"
        }
    }

    fn counterparty_label(&self) -> &'static str {
        match self {
            InvoiceType::Sales => "Customer",
            InvoiceType::Purchases => "Seller"
        }
    }
}

pub async fn list_sales_invoices(app_user: &AppUser, from: String, to: String) -> anyhow::Result<()> {
    browse_invoices(app_user, &InvoiceType::Sales, from, to).await
}

pub async fn list_purchase_invoices(app_user: &AppUser, from: String, to: String) -> anyhow::Result<()> {
    browse_invoices(app_user, &InvoiceType::Purchases, from, to).await
}

async fn browse_invoices(app_user: &AppUser, invoice_type: &InvoiceType, from: String, to: String) -> anyhow::Result<()> {
    let invoices = list_invoices(app_user, invoice_type, &from, &to).await?;
    if invoices.is_empty() {
        println!();
        return Ok(());
    }
    let counterparty_key = match invoice_type { InvoiceType::Sales => "Buyer", InvoiceType::Purchases => "Seller" };
    let counterparty_label = invoice_type.counterparty_label();
    let invoice_number_width = invoices.iter().map(|i| i["InvoiceBody"]["InvoiceNumber"].as_str().unwrap().len()).max().unwrap_or(0);
    let counterparty_width = invoices.iter().map(|i| i[counterparty_key]["IdentificationData"]["Name"].as_str().unwrap().len()).max().unwrap_or(0);
    let mut invoice_choices: Vec<String> = invoices.iter().enumerate().map(|(index, invoice)| {
        let number = invoice["InvoiceBody"]["InvoiceNumber"].as_str().unwrap();
        let counterparty = invoice[counterparty_key]["IdentificationData"]["Name"].as_str().unwrap();
        let amount = invoice["InvoiceBody"]["TotalGrossAmount"].as_f64().unwrap();
        let currency = invoice["InvoiceBody"]["CurrencyCode"].as_str().unwrap();
        format!("{}. {:<invoice_number_width$} - {}: {:<counterparty_width$} - {:.2} {}", index + 1, number, counterparty_label, counterparty, amount, currency)
    }).collect();
    invoice_choices.push("Back ↩️".to_string());
    let selected = Select::new(&format!("Select a {invoice_type} invoice"), invoice_choices).prompt()?;
    if selected == "Back ↩️" {
        return Ok(());
    }
    let index = selected.split_once(". ").unwrap().0.parse::<usize>()? - 1;
    let invoice = &invoices[index];
    let invoice_number = invoice["InvoiceBody"]["InvoiceNumber"].as_str().unwrap().to_string();
    let action = Select::new(&format!("Invoice {invoice_number}"), vec![
        "👀 Preview",
        "📂 Download XML",
        "Back ↩️"
    ]).prompt()?;
    match action {
        "👀 Preview" => {
            print_invoice_preview(invoice, invoice_type);
            crate::tui::inquire::pause()?;
        }
        "📂 Download XML" => {
            let path = download_invoice_xml(app_user, invoice_type, invoice, &invoice_number, &from, &to).await?;
            println!("  📂 Invoice XML saved to {}", path.display().to_string().dark_yellow());
            crate::tui::inquire::pause()?;
        }
        _ => {}
    }
    Ok(())
}

fn print_invoice_preview(invoice: &serde_json::Value, invoice_type: &InvoiceType) {
    println!();
    let body = &invoice["InvoiceBody"];
    let counterparty_key = match invoice_type { InvoiceType::Sales => "Buyer", InvoiceType::Purchases => "Seller" };
    let counterparty = &invoice[counterparty_key]["IdentificationData"];
    let currency = body["CurrencyCode"].as_str().unwrap();
    println!("  InvoiceNumber: {}", body["InvoiceNumber"].as_str().unwrap());
    println!("  InvoiceType: {}", body["InvoiceType"].as_str().unwrap());
    println!("  {}: {} - {}", invoice_type.counterparty_label(), counterparty["NIP"].as_str().unwrap_or("-"), counterparty["Name"].as_str().unwrap());
    println!("  ServiceDate: {}", body["ServiceDate"].as_str().unwrap());
    println!("  TotalGrossAmount: {:.2} {}", body["TotalGrossAmount"].as_f64().unwrap(), currency);
    println!("  TotalNetAmount: {:.2} {}", body["TotalNetAmount"].as_f64().unwrap(), currency);
    println!("  TotalVatAmount: {:.2} {}", body["TotalVatAmount"].as_f64().unwrap(), currency);
}

async fn list_invoices(app_user: &AppUser, endpoint: &InvoiceType, from: &str, to: &str) -> anyhow::Result<Vec<serde_json::Value>> {
    let json: serde_json::Value = reqwest::Client::new()
        .get(format!("{}/ksef/{endpoint}", cf_worker_url!()))
        .query(&[
            ("from", from),
            ("to", to),
        ])
        .header("CF-Access-Client-Id", cf_client_id!())
        .header("CF-Access-Client-Secret", cf_client_secret!())
        .header("X-API-Key", app_user.api_key.as_deref().ok_or_else(|| anyhow::anyhow!("User has no API key configured"))?)
        .header("X-User-Id", &app_user.id)
        .header("Accept", "application/json")
        .send()
        .await?
        .json()
        .await?;
    if json["success"].as_bool() != Some(true) {
        println!("  API Response: {}", json["error"].as_str().unwrap_or("unknown error"));
        return Ok(Vec::new());
    }
    let invoices = json["result"].as_array().cloned().unwrap();
    println!("  API Response: {} invoices [{} {}] found", invoices.len(), endpoint.emoji(), endpoint);
    Ok(invoices)
}

async fn download_invoice_xml(app_user: &AppUser, invoice_type: &InvoiceType, invoice: &serde_json::Value, invoice_number: &str, from: &str, to: &str) -> anyhow::Result<PathBuf> {
    let mut url = reqwest::Url::parse(&format!("{}/ksef/{invoice_type}", cf_worker_url!()))?;
    url.query_pairs_mut()
        .append_pair("invoiceNumber", invoice_number)
        .append_pair("from", from)
        .append_pair("to", to);
    let response = reqwest::Client::new()
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
    if !content_type.to_ascii_lowercase().contains("xml") || !xml.trim_start().starts_with("<?xml")
    {
        anyhow::bail!("Expected invoice XML, but the server returned {content_type}.");
    }
    let issue_date = invoice["InvoiceBody"]["IssueDate"].as_str()
        .and_then(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
        .ok_or_else(|| anyhow::anyhow!("Invoice is missing a valid issue date"))?;
    let safe_number: String = invoice_number.chars().map(|character| {
        if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
            character
        } else {
            '_'
        }
    }).collect();
    let home = std::env::var_os("USERPROFILE").unwrap();
    let download_folder = PathBuf::from(home)
        .join(".ksefbot")
        .join("downloads")
        .join(invoice_type.to_string())
        .join(issue_date.format("%Y").to_string())
        .join(issue_date.format("%B").to_string());
    std::fs::create_dir_all(&download_folder)?;
    let path = download_folder.join(format!("{safe_number}.xml"));
    std::fs::write(&path, xml)?;
    Ok(path)
}