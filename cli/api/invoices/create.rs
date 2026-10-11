use crate::api::customers::{AppContractor, load_contractors};
use crate::api::users::AppUser;
use crate::tui::inquire::invoices_tui::InvoiceInput;
use crate::{cf_client_id, cf_client_secret, cf_worker_url};
use chrono::{Datelike, NaiveDate};
use std::path::PathBuf;
use xmltree::{Element, XMLNode};

pub struct InvoiceParties {
    pub user_contractor: AppContractor,
    pub customers: Vec<AppContractor>
}

pub struct SalesInvoice {
    pub invoice_number: String,
    pub month_name: String,
    pub year: i32,
    pub xml: String,
    pub submission: Option<KsefSubmissionReferences>
}

pub struct SalesInvoicePreview {
    pub total_net: f64,
    pub total_vat: f64,
    pub total_gross: f64
}

pub enum UploadInvoiceResult {
    Uploaded(String),
    AlreadyExists(String)
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KsefSubmissionReferences {
    pub session_reference_number: String,
    pub invoice_reference_number: String
}

impl SalesInvoice {
    pub fn from_xml(xml: String) -> anyhow::Result<Self> {
        let root = Element::parse(xml.as_bytes())?;
        let fa = root.get_child("Fa").unwrap();
        let invoice_number = fa.get_child("P_2").unwrap().get_text().unwrap().to_string();
        let issue_date = NaiveDate::parse_from_str(&fa.get_child("P_1").unwrap().get_text().unwrap(), "%Y-%m-%d")?;
        Ok(Self {
            invoice_number,
            month_name: issue_date.format("%B").to_string().to_lowercase(),
            year: issue_date.year(),
            xml,
            submission: None
        })
    }
}

pub async fn load_invoice_parties(app_user: &AppUser) -> anyhow::Result<InvoiceParties> {
    let contractors = load_contractors(app_user).await?;
    let user_contractor = contractors.iter()
        .find(|contractor| app_user.contractor_id.as_deref() == Some(&contractor.id))
        .or_else(|| {
            contractors.iter().find(|contractor| contractor.nip.as_deref() == Some(&app_user.id))
        })
        .cloned()
        .ok_or_else(|| {
            anyhow::anyhow!("Seller details not found in the contractor database")
        })?;
    let customers: Vec<AppContractor> = contractors.into_iter().filter(|contractor| {
        contractor.id != user_contractor.id
    }).collect();
    if customers.is_empty() {
        anyhow::bail!("No customers found. Add a customer before creating an invoice.");
    }
    Ok(InvoiceParties { user_contractor, customers })
}

pub fn preview_sales_invoice(invoice: &SalesInvoice) -> anyhow::Result<SalesInvoicePreview> {
    let root = Element::parse(invoice.xml.as_bytes())?;
    let fa = root.get_child("Fa").unwrap();
    let sum = |prefix: &str| -> anyhow::Result<f64> {
        fa.children.iter().filter_map(XMLNode::as_element)
            .filter(|element| element.name.starts_with(prefix))
            .map(|element| Ok(element.get_text().unwrap().parse::<f64>()?))
            .sum()
    };
    Ok(SalesInvoicePreview {
        total_net: sum("P_13_")?,
        total_vat: sum("P_14_")?,
        total_gross: fa.get_child("P_15").unwrap().get_text().unwrap().parse()?
    })
}

pub async fn create_invoice(app_user: &AppUser, input: &InvoiceInput) -> anyhow::Result<SalesInvoice> {
    let response = crate::api::client::http_client()
        .post(format!("{}/app/invoices/generate", cf_worker_url!()))
        .header("CF-Access-Client-Id", cf_client_id!())
        .header("CF-Access-Client-Secret", cf_client_secret!())
        .header("X-API-Key", app_user.api_key.as_deref().ok_or_else(|| anyhow::anyhow!("User has no API key configured"))?)
        .header("X-User-Id", &app_user.id)
        .header("Accept", "application/xml")
        .json(input)
        .send()
        .await?;
    let response = ensure_success(response, "Invoice generation").await?;
    SalesInvoice::from_xml(response.text().await?)
}

pub async fn upload_invoice(app_user: &AppUser, invoice: &SalesInvoice, notes: &str) -> anyhow::Result<UploadInvoiceResult> {
    let file = reqwest::multipart::Part::bytes(invoice.xml.as_bytes().to_vec()).file_name("invoice.xml").mime_str("application/xml")?;
    let form = reqwest::multipart::Form::new().part("file", file).text("type", "sales").text("notes", notes.to_string());
    let response = crate::api::client::http_client()
        .post(format!("{}/app/invoices/ingest", cf_worker_url!()))
        .header("CF-Access-Client-Id", cf_client_id!())
        .header("CF-Access-Client-Secret", cf_client_secret!())
        .header("X-API-Key", app_user.api_key.as_deref().ok_or_else(|| anyhow::anyhow!("User has no API key configured"))?)
        .header("X-User-Id", &app_user.id)
        .header("Accept", "application/json")
        .multipart(form)
        .send()
        .await?;
    if response.status() == reqwest::StatusCode::CONFLICT {
        let body: serde_json::Value = response.json().await?;
        return Ok(UploadInvoiceResult::AlreadyExists(
            body["error"].as_str().unwrap_or("✅ Invoice already exists").to_owned()
        ));
    }
    let response = ensure_success(response, "📤 Invoice uploaded").await?;
    let body: serde_json::Value = response.json().await?;
    if body["success"].as_bool() != Some(true) {
        anyhow::bail!("Invoice upload failed: {}", body["error"].as_str().unwrap_or("unknown error"));
    }
    Ok(UploadInvoiceResult::Uploaded(body["id"].as_str().unwrap().to_owned()))
}

pub async fn submit_invoice(app_user: &AppUser, invoice: &SalesInvoice) -> anyhow::Result<KsefSubmissionReferences> {
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(invoice.xml.as_bytes().to_vec()).file_name("invoice.xml").mime_str("application/xml")?
    );
    let response = crate::api::client::http_client()
        .post(format!("{}/ksef/sales", cf_worker_url!()))
        .header("CF-Access-Client-Id", cf_client_id!())
        .header("CF-Access-Client-Secret", cf_client_secret!())
        .header("X-API-Key", app_user.api_key.as_deref().ok_or_else(|| anyhow::anyhow!("User has no API key configured"))?)
        .header("X-User-Id", &app_user.id)
        .header("Accept", "application/json")
        .multipart(form)
        .send()
        .await?;
    let response = ensure_success(response, "KSeF submission").await?;
    let mut body: serde_json::Value = response.json().await?;
    if body["success"].as_bool() != Some(true) {
        anyhow::bail!("KSeF submission failed: {}", body["error"].as_str().unwrap_or("unknown error"));
    }
    Ok(serde_json::from_value(body["result"].take())?)
}

pub async fn download_receipt(app_user: &AppUser, invoice: &SalesInvoice) -> anyhow::Result<PathBuf> {
    let mut url = reqwest::Url::parse(&format!("{}/ksef/sales/receipt", cf_worker_url!()))?;
    url.query_pairs_mut()
        .append_pair("sessionReferenceNumber", &invoice.submission.as_ref().unwrap().session_reference_number)
        .append_pair("invoiceReferenceNumber", &invoice.submission.as_ref().unwrap().invoice_reference_number);
    let response = crate::api::client::http_client().get(url)
        .header("CF-Access-Client-Id", cf_client_id!())
        .header("CF-Access-Client-Secret", cf_client_secret!())
        .header("X-API-Key", app_user.api_key.as_deref().ok_or_else(|| anyhow::anyhow!("User has no API key configured"))?)
        .header("X-User-Id", &app_user.id)
        .header("Accept", "application/xml")
        .send()
        .await?;
    let response = ensure_success(response, "KSeF receipt download").await?;
    let safe_number: String = invoice.invoice_number.chars().map(|character| {
        if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
            character
        } else {
            '_'
        }
    }).collect();
    let home = std::env::var_os("USERPROFILE").unwrap();
    let submitted_folder = PathBuf::from(home).join(".ksefbot").join("submitted");
    std::fs::create_dir_all(&submitted_folder)?;
    let path = submitted_folder.join(format!("{safe_number}-UPO.xml"));
    std::fs::write(&path, response.bytes().await?)?;
    Ok(path)
}

async fn ensure_success(response: reqwest::Response, operation: &str) -> anyhow::Result<reqwest::Response> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let body = response.text().await?;
    anyhow::bail!("{operation} failed with {status}: {body}")
}
