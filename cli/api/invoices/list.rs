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
    #[strum(to_string = "sales")] Sales,
    #[strum(to_string = "purchases")] Purchases
}

impl InvoiceType {
    fn counterparty_label(&self) -> &'static str {
        match self {
            InvoiceType::Sales => "Customer",
            InvoiceType::Purchases => "Seller"
        }
    }

    fn emoji(&self) -> &'static str {
        match self {
            InvoiceType::Sales => "💵",
            InvoiceType::Purchases => "🛒"
        }
    }
}

pub async fn list_sales_invoices(app_user: &AppUser, from: String, to: String) -> anyhow::Result<()> {
    let sales_invoices = fetch_invoices(app_user, &InvoiceType::Sales, &from, &to).await?;
    browse_invoices(&InvoiceType::Sales, sales_invoices).await
}

pub async fn list_purchase_invoices(app_user: &AppUser, from: String, to: String) -> anyhow::Result<()> {
    let purchase_invoices = fetch_invoices(app_user, &InvoiceType::Purchases, &from, &to).await?;
    browse_invoices(&InvoiceType::Purchases, purchase_invoices).await
}

pub async fn sales_invoice_months(app_user: &AppUser, year: i32) -> anyhow::Result<[bool; 12]> {
    Ok(crate::api::client::http_client()
        .get(format!("{}/app/invoices/monthly", cf_worker_url!()))
        .query(&[("year", year)])
        .header("CF-Access-Client-Id", cf_client_id!())
        .header("CF-Access-Client-Secret", cf_client_secret!())
        .header("X-API-Key", app_user.api_key.as_deref().ok_or_else(|| anyhow::anyhow!("User has no API key configured"))?)
        .header("X-User-Id", &app_user.id)
        .header("Accept", "application/json")
        .send().await?.error_for_status()?.json().await?)
}

async fn browse_invoices(invoice_type: &InvoiceType, invoices: Vec<serde_json::Value>) -> anyhow::Result<()> {
    if invoices.is_empty() {
        crate::tui::inquire::pause()?;
        return Ok(());
    }
    let counterparty_key = match invoice_type { InvoiceType::Sales => "Buyer", InvoiceType::Purchases => "Seller" };
    let capitalize = |text: &str| -> String {
        let mut chars = text.chars();
        match chars.next() {
            Some(first) => first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase(),
            None => String::new()
        }
    };
    let item_description = |invoice: &serde_json::Value| -> String {
        invoice_data(invoice)["InvoiceBody"]["InvoiceLines"].as_array().into_iter().flatten()
            .filter_map(|line| line["ItemDescription"].as_str())
            .map(str::trim)
            .map(&capitalize)
            .collect::<Vec<_>>()
            .join(", ")
    };
    let invoice_number_width = invoices.iter().map(|i| invoice_data(i)["InvoiceBody"]["InvoiceNumber"].as_str().unwrap().len()).max().unwrap_or(0);
    let counterparty_width = invoices.iter().map(|i| invoice_data(i)[counterparty_key]["IdentificationData"]["Name"].as_str().unwrap().len()).max().unwrap_or(0);
    let item_description_width = invoices.iter().map(item_description).map(|d| d.len()).max().unwrap_or(0);
    let mut invoice_choices: Vec<String> = invoices.iter().enumerate().map(|(index, invoice)| {
        let data = invoice_data(invoice);
        let number = data["InvoiceBody"]["InvoiceNumber"].as_str().unwrap();
        let counterparty = data[counterparty_key]["IdentificationData"]["Name"].as_str().unwrap();
        let items = item_description(invoice);
        let amount = data["InvoiceBody"]["TotalGrossAmount"].as_f64().unwrap();
        let currency = data["InvoiceBody"]["CurrencyCode"].as_str().unwrap();
        let amount_text = format!("{amount:.2} {currency}");
        let amount_styled = match invoice_type {
            InvoiceType::Sales => amount_text.blue().to_string(),
            InvoiceType::Purchases => amount_text.dark_yellow().to_string()
        };
        let number_padded = format!("{number:<invoice_number_width$}");
        format!("{}. {} - {:<counterparty_width$} - {:<item_description_width$} - {}", index + 1, number_padded.bold(), counterparty, items, amount_styled)
    }).collect();
    invoice_choices.push("Back ↩️".to_string());
    let selected = Select::new(&format!("Select a {invoice_type} invoice"), invoice_choices).with_page_size(15).prompt()?;
    if selected == "Back ↩️" {
        return Ok(());
    }
    let index = selected.split_once(". ").unwrap().0.parse::<usize>()? - 1;
    let invoice = &invoices[index];
    let invoice_number = invoice_data(invoice)["InvoiceBody"]["InvoiceNumber"].as_str().unwrap().to_string();
    let action = Select::new(&format!("Invoice {}", invoice_number.clone().bold()), vec![
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
            let path = download_invoice_xml(invoice_type, invoice, &invoice_number)?;
            println!("  📂 Invoice XML saved to {}", path.display().to_string().dark_yellow());
            crate::tui::inquire::pause()?;
        }
        _ => {}
    }
    Ok(())
}

fn print_invoice_preview(invoice: &serde_json::Value, invoice_type: &InvoiceType) {
    println!();
    let invoice_data = invoice_data(invoice);
    let body = &invoice_data["InvoiceBody"];
    let counterparty_key = match invoice_type { InvoiceType::Sales => "Buyer", InvoiceType::Purchases => "Seller" };
    let counterparty = &invoice_data[counterparty_key]["IdentificationData"];
    let currency = body["CurrencyCode"].as_str().unwrap();
    let gross = format!("{:.2} {}", body["TotalGrossAmount"].as_f64().unwrap(), currency);
    let net = format!("{:.2} {}", body["TotalNetAmount"].as_f64().unwrap(), currency);
    let vat = format!("{:.2} {}", body["TotalVatAmount"].as_f64().unwrap(), currency);
    let (gross, net) = match invoice_type {
        InvoiceType::Sales => (gross.blue().to_string(), net.green().to_string()),
        InvoiceType::Purchases => (gross.dark_yellow().to_string(), net)
    };
    let vat = match invoice_type {
        InvoiceType::Purchases => vat.green().to_string(),
        InvoiceType::Sales => vat
    };
    println!("  InvoiceNumber: {}", body["InvoiceNumber"].as_str().unwrap().bold());
    println!("  InvoiceType: {}", body["InvoiceType"].as_str().unwrap());
    println!("  {}: {} - {}", invoice_type.counterparty_label(), counterparty["NIP"].as_str().unwrap_or("-"), counterparty["Name"].as_str().unwrap());
    println!("  ServiceDate: {}", body["ServiceDate"].as_str().unwrap());
    println!("  TotalGrossAmount: {gross}");
    println!("  TotalNetAmount: {net}");
    println!("  TotalVatAmount: {vat}");
}

async fn fetch_invoices(app_user: &AppUser, endpoint: &InvoiceType, from: &str, to: &str) -> anyhow::Result<Vec<serde_json::Value>> {
    let mut json: serde_json::Value = crate::api::client::http_client()
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
    let invoices = match json["result"].take() {
        serde_json::Value::Array(array) => array,
        _ => Vec::new()
    };
    println!(
        "  API Response: {} invoices [{} {}] found ({} existing, {} synced)",
        invoices.len(),
        endpoint.emoji(),
        endpoint,
        json["counts"]["fromDb"].as_u64().unwrap(),
        json["counts"]["fromKsef"].as_u64().unwrap()
    );
    parse_invoice_rows(invoices)
}

fn parse_invoice_rows(rows: Vec<serde_json::Value>) -> anyhow::Result<Vec<serde_json::Value>> {
    rows.into_iter().map(|mut row| {
        if let Some(json_data) = row.get("jsonData").and_then(serde_json::Value::as_str).map(str::to_owned) {
            let parsed = serde_json::from_str(&json_data)?;
            row["jsonData"] = parsed;
        }
        Ok(row)
    }).collect()
}

pub fn invoice_data(invoice: &serde_json::Value) -> &serde_json::Value {
    invoice.get("jsonData").filter(|data| data.is_object()).unwrap()
}

fn download_invoice_xml(invoice_type: &InvoiceType, invoice: &serde_json::Value, invoice_number: &str) -> anyhow::Result<PathBuf> {
    let xml = invoice["rawXml"].as_str().ok_or_else(|| anyhow::anyhow!("Invoice missing XML content"))?;
    let issue_date = invoice_data(invoice)["InvoiceBody"]["IssueDate"].as_str()
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