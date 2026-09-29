use crate::api::users::AppUser;
use crate::{cf_client_id, cf_client_secret, cf_worker_url};
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
    let invoices = list_invoices(app_user, &InvoiceType::Sales, from, to).await?;
    let max_width = |get: fn(&serde_json::Value) -> &str| {
        invoices.iter().map(get).map(str::len).max().unwrap_or(0)
    };
    let invoice_number_width = max_width(|i| i["InvoiceBody"]["InvoiceNumber"].as_str().unwrap());
    let seller_width = max_width(|i| i["Seller"]["IdentificationData"]["Name"].as_str().unwrap());
    for (i, invoice) in invoices.iter().enumerate() {
        let invoice_number = invoice["InvoiceBody"]["InvoiceNumber"].as_str().unwrap();
        let seller = invoice["Seller"]["IdentificationData"]["Name"].as_str().unwrap();
        let total = invoice["InvoiceBody"]["TotalGrossAmount"].as_f64().unwrap();
        let currency = invoice["InvoiceBody"]["CurrencyCode"].as_str().unwrap();
        println!("  {}. {:<invoice_number_width$} - {:<seller_width$} - {:.2} {}", i + 1, invoice_number, seller, total, currency);
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
    let api_key = app_user.api_key.as_deref().ok_or_else(|| anyhow::anyhow!("The application user has no API key configured"))?;
    let json: serde_json::Value = reqwest::Client::new()
        .get(format!("{}/ksef/{endpoint}", cf_worker_url!()))
        .query(&[
            ("from", from),
            ("to", to),
        ])
        .header("CF-Access-Client-Id", cf_client_id!())
        .header("CF-Access-Client-Secret", cf_client_secret!())
        .header("X-API-Key", api_key)
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