use crate::api::users::AppUser;
use crate::{cf_client_id, cf_client_secret, cf_worker_url};
use crossterm::style::Stylize;
use inquire::{Confirm, Select};
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

pub async fn list_sales_invoices(app_user: &AppUser, from: String, to: String) -> anyhow::Result<()> {
    let invoices = list_invoices(app_user, &InvoiceType::Sales, from.clone(), to.clone()).await?;
    if invoices.is_empty() {
        return Ok(());
    }
    let mut choices: Vec<String> = invoices.iter().enumerate().map(|(index, invoice)| {
        let number = invoice["InvoiceBody"]["InvoiceNumber"].as_str().unwrap();
        let amount = invoice["InvoiceBody"]["TotalGrossAmount"].as_f64().unwrap();
        let currency = invoice["InvoiceBody"]["CurrencyCode"].as_str().unwrap();
        format!("{}. {} - {:.2} {}", index + 1, number, amount, currency)
    }).collect();
    choices.push("Back ↩️".to_string());
    let selected = Select::new("Select a sales invoice", choices).prompt()?;
    if selected == "Back ↩️" {
        return Ok(());
    }
    let index = selected.split_once(". ").unwrap().0.parse::<usize>().unwrap() - 1;
    let invoice_number = invoices[index]["InvoiceBody"]["InvoiceNumber"].as_str().unwrap();
    if Confirm::new(&format!("Download invoice {invoice_number}?")).with_default(true).prompt()? {
        let path = download_sales_invoice(app_user, invoice_number, &from, &to).await?;
        println!("  📂 Invoice XML saved to {}", path.display().to_string().dark_yellow());
    }
    Ok(())
}

pub async fn list_purchase_invoices(app_user: &AppUser, from: String, to: String) -> anyhow::Result<()> {
    let invoices = list_invoices(app_user, &InvoiceType::Purchases, from, to).await?;
    let max_width = |get: fn(&serde_json::Value) -> &str| {
        invoices.iter().map(get).map(str::len).max().unwrap_or(0)
    };
    let invoice_number_width = max_width(|i| i["InvoiceBody"]["InvoiceNumber"].as_str().unwrap());
    let seller_width = max_width(|i| i["Seller"]["IdentificationData"]["Name"].as_str().unwrap());
    for (i, invoice) in invoices.iter().enumerate() {
        let invoice_number = invoice["InvoiceBody"]["InvoiceNumber"].as_str().unwrap();
        let seller = invoice["Seller"]["IdentificationData"]["Name"]
            .as_str()
            .unwrap();
        let total = invoice["InvoiceBody"]["TotalGrossAmount"].as_f64().unwrap();
        let currency = invoice["InvoiceBody"]["CurrencyCode"].as_str().unwrap();
        println!("  {}. {:<invoice_number_width$} - {:<seller_width$} - {:.2} {}", i + 1, invoice_number, seller, total, currency);
    }
    Ok(())
}

async fn list_invoices(app_user: &AppUser, endpoint: &InvoiceType, from: String, to: String) -> anyhow::Result<Vec<serde_json::Value>> {
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
    println!("  API Response: {} {} invoices found", endpoint, invoices.len());
    Ok(invoices)
}

async fn download_sales_invoice(app_user: &AppUser, invoice_number: &str, from: &str, to: &str) -> anyhow::Result<PathBuf> {
    let mut url = reqwest::Url::parse(&format!("{}/ksef/sales", cf_worker_url!()))?;
    url.query_pairs_mut()
        .append_pair("from", from)
        .append_pair("to", to)
        .append_pair("invoiceNumber", invoice_number)
        .append_pair("format", "xml");
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
    let xml = response.text().await?;
    let safe_number: String = invoice_number.chars().map(|character| {
        if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
            character
        } else {
            '_'
        }
    }).collect();
    let home = std::env::var_os("USERPROFILE").unwrap();
    let download_folder = PathBuf::from(home).join(".ksefbot").join("downloaded");
    std::fs::create_dir_all(&download_folder)?;
    let path = download_folder.join(format!("{safe_number}.xml"));
    std::fs::write(&path, xml)?;
    Ok(path)
}