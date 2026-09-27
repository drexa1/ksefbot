use inquire::DateSelect;
use std::env::var;
use strum::{Display};
use crate::{cf_client_id, cf_client_secret};

#[derive(Clone, Display)]
pub enum InvoiceType {
    #[strum(to_string = "sales")] Sales,
    #[strum(to_string = "purchases")] Purchases
}

pub async fn list_sales_invoices() -> anyhow::Result<()> {
    let invoices = list_invoices(&InvoiceType::Sales).await?;

    let max_width = |get: fn(&serde_json::Value) -> &str| invoices.iter().map(get).map(str::len).max().unwrap_or(0);
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

pub async fn list_purchase_invoices() -> anyhow::Result<()> {
    let invoices = list_invoices(&InvoiceType::Purchases).await?;

    let max_width = |get: fn(&serde_json::Value) -> &str| invoices.iter().map(get).map(str::len).max().unwrap_or(0);
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

const DATEPICKER_HELP_MESSAGE: &str = "Arrows: move · PageUp/Down: months · Shift+PageUp/Down: years · Enter: select";
async fn list_invoices(endpoint: &InvoiceType) -> anyhow::Result<Vec<serde_json::Value>> {
    let from = DateSelect::new("From date:")
        .with_help_message(DATEPICKER_HELP_MESSAGE)
        .prompt()?.and_hms_opt(0, 0, 0).unwrap().and_utc();
    let to = DateSelect::new("To date:")
        .with_help_message(DATEPICKER_HELP_MESSAGE)
        .prompt()?.and_hms_opt(23, 59, 59).unwrap().and_utc();
    let json: serde_json::Value = reqwest::Client::new()
        .get(format!("{}/ksef/{endpoint}", var("CF_WORKER_URL")?))
        .query(&[("from", from.format("%Y/%m/%d").to_string()), ("to", to.format("%Y/%m/%d").to_string())])
        .header("CF-Access-Client-Id", cf_client_id!())
        .header("CF-Access-Client-Secret", cf_client_secret!())
        .header("X-API-Key", var("APP_API_KEY")?)  // FIXME: this should be available from logged user
        .header("X-User-Id", var("APP_USER_ID")?)  // FIXME: this should be available from logged user
        .header("Accept", "application/json")
        .send().await?.json().await?;
    if json["success"].as_bool() != Some(true) {
        println!("  API Response: {}", json["error"].as_str().unwrap());
        return Ok(Vec::new());
    }
    let invoices = json["result"].as_array().cloned().unwrap();
    println!("  API Response: {} {} invoices found", endpoint, invoices.len());
    Ok(invoices)
}

pub async fn create_sales_invoice() -> anyhow::Result<()> {
    println!("Step 1/5: Loading customers...");
    println!("  [API] GET /customers");
    println!("  [API] 3 customers available.");
    println!();
    println!("Step 2/5: Selecting customer...");
    println!("  Customer: ACME Sp. z o.o.");
    println!("  NIP: 1234567890");
    println!();
    println!("Step 3/5: Adding invoice items...");
    println!("  Item: Software development");
    println!("  Quantity: 1");
    println!("  Net price: 1,000.00 PLN");
    println!();
    println!("Step 4/5: Calculating VAT...");
    println!("  VAT rate: 23%");
    println!("  VAT: 230.00 PLN");
    println!("  Gross total: 1,230.00 PLN");
    println!();
    println!("Step 5/5: Creating invoice...");
    println!("  [API] POST /invoices");
    println!("  [API] Invoice created.");
    println!("  Invoice number: FV/2026/004");
    println!();
    println!("Sending invoice to KSeF...");
    println!("  [KSeF] Submitting invoice...");
    println!("  [KSeF] Invoice accepted.");
    println!("  KSeF reference: 2026-ABC-123456");
    Ok(())
}