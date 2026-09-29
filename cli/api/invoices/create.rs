use crate::api::customers::{AppContractor, load_contractors};
use crate::api::users::AppUser;
use crate::{cf_client_id, cf_client_secret, cf_worker_url};
use chrono::{Datelike, Duration, Local, NaiveDate, SecondsFormat, Utc};
use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::PathBuf;

const DEFAULT_VAT_RATE: f64 = 0.23;
const DEFAULT_PAYMENT_TYPE: &str = "6";
const DEFAULT_PAYMENT_TERM_DAYS: i64 = 7;
const INVOICE_TEMPLATE: &str = include_str!("../../../public/schemas/invoice-template.xml");

pub struct InvoiceParties {
    pub seller: AppContractor,
    pub customers: Vec<AppContractor>
}

pub struct SalesInvoice {
    pub invoice_number: String,
    pub month_name: String,
    pub xml: String,
    pub session_reference_number: Option<String>,
    pub invoice_reference_number: Option<String>
}

pub struct SalesInvoicePreview {
    pub hourly_rate: f64,
    pub total_net: f64,
    pub total_vat: f64
}

struct InvoiceData {
    number: String,
    issue_date: NaiveDate,
    delivery_date: NaiveDate,
    payment_deadline: NaiveDate,
    item_name: String,
    hours_worked: u32,
    hourly_rate: f64,
    total_net: f64,
    total_vat: f64,
    total_gross: f64
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

pub async fn load_invoice_parties(app_user: &AppUser) -> anyhow::Result<InvoiceParties> {
    let contractors = load_contractors(app_user).await?;
    let seller = contractors.iter()
        .find(|contractor| app_user.contractor_id.as_deref() == Some(&contractor.id))
        .or_else(|| {
            contractors.iter().find(|contractor| contractor.nip.as_deref() == Some(&app_user.id))
        })
        .cloned()
        .ok_or_else(|| {
            anyhow::anyhow!("Your seller details were not found in the contractor database")
        })?;
    if seller.nip.as_deref().is_none_or(str::is_empty) {
        anyhow::bail!("Your seller profile must include a NIP before creating an invoice");
    }
    let customers = contractors.into_iter().filter(|contractor| {
        contractor.id != seller.id && contractor.nip.as_deref().is_some_and(|nip| !nip.is_empty())
    }).collect();
    Ok(InvoiceParties { seller, customers })
}

pub async fn create_invoice(app_user: &AppUser, seller: &AppContractor, customer: &AppContractor, hours_worked: u32) -> anyhow::Result<SalesInvoice> {
    if hours_worked <= 0 {
        anyhow::bail!("Hours worked must be greater than zero");
    }
    let invoice = InvoiceData::with_defaults(app_user, hours_worked)?;
    let xml = invoice.to_xml(seller, customer, app_user.bank_account_number.as_deref())?;
    Ok(SalesInvoice {
        invoice_number: invoice.number,
        month_name: invoice.issue_date.format("%B").to_string().to_lowercase(),
        xml,
        session_reference_number: None,
        invoice_reference_number: None
    })
}

pub fn preview_sales_invoice(app_user: &AppUser, hours_worked: u32) -> anyhow::Result<SalesInvoicePreview> {
    let invoice = InvoiceData::with_defaults(app_user, hours_worked)?;
    Ok(SalesInvoicePreview { hourly_rate: invoice.hourly_rate, total_net: invoice.total_net, total_vat: invoice.total_vat })
}

impl InvoiceData {
    fn with_defaults(app_user: &AppUser, hours_worked: u32) -> anyhow::Result<Self> {
        if hours_worked <= 0 {
            anyhow::bail!("Hours worked must be greater than zero");
        }
        let today = Local::now().date_naive();
        let hourly_rate = app_user.default_hourly_rate.ok_or_else(|| anyhow::anyhow!("Default hourly rate is not configured"))?;
        if !hourly_rate.is_finite() || hourly_rate <= 0.0 {
            anyhow::bail!("Default hourly rate must be greater than zero");
        }
        let net_unrounded = hourly_rate * f64::from(hours_worked);
        let vat_unrounded = net_unrounded * DEFAULT_VAT_RATE;
        if !net_unrounded.is_finite() || !vat_unrounded.is_finite() || net_unrounded.abs() > f64::MAX / 100.0 || vat_unrounded.abs() > f64::MAX / 100.0 {
            anyhow::bail!("Invoice total is too large");
        }
        let total_net = round_money(net_unrounded);
        let total_vat = round_money(vat_unrounded);
        let total_gross = round_money(total_net + total_vat);
        if !total_gross.is_finite() {
            anyhow::bail!("Invoice total is too large");
        }
        let year = today.year();
        let month = today.month();
        let posting_date = NaiveDate::from_ymd_opt(year, month, 1)
            .and_then(|date| date.checked_add_months(chrono::Months::new(1)))
            .and_then(|date| date.pred_opt())
            .ok_or_else(|| anyhow::anyhow!("Could not determine the current month's end date"))?;
        let payment_deadline = posting_date.checked_add_signed(Duration::days(DEFAULT_PAYMENT_TERM_DAYS)).ok_or_else(|| {
            anyhow::anyhow!("Payment deadline is outside the supported date range")
        })?;
        Ok(Self {
            number: format!("eFA/{year}/{month:02}/1"),
            issue_date: today,
            delivery_date: posting_date,
            payment_deadline,
            item_name: app_user.default_item_name.clone().filter(|name| !name.trim().is_empty()).unwrap_or_else(|| "Consulting services".to_string()),
            hours_worked,
            hourly_rate,
            total_net,
            total_vat,
            total_gross
        })
    }

    fn to_xml(&self, seller: &AppContractor, customer: &AppContractor, bank_account: Option<&str>) -> anyhow::Result<String> {
        let mut invoice_line = String::new();
        let _ = write!(invoice_line,
            "<FaWiersz><NrWierszaFa>1</NrWierszaFa><P_7>{}</P_7><P_8A>hour</P_8A>\
             <P_8B>{}</P_8B><P_9A>{:.2}</P_9A><P_11>{:.2}</P_11><P_11Vat>{:.2}</P_11Vat>\
             <P_12>23</P_12></FaWiersz>",
            xml_escape(&self.item_name),
            self.hours_worked,
            self.hourly_rate,
            self.total_net,
            self.total_vat
        );
        let bank_account = bank_account
            .filter(|account| !account.trim().is_empty())
            .map(|account| format!("<RachunekBankowy><NrRB>{}</NrRB></RachunekBankowy>", xml_escape(account)))
            .unwrap_or_default();
        let values = [
            ("FORM_CODE", "FA".to_string()),
            ("GENERATION_DATE", Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)),
            ("SYSTEM_INFO", "KSeF Bot".to_string()),
            ("CONTRACTOR_NIP", xml_escape(seller.nip.as_deref().unwrap_or_default())),
            ("CONTRACTOR_NAME", xml_escape(&seller.name)),
            ("COUNTRY_CODE", xml_escape(&seller.country_code)),
            ("CONTRACTOR_ADDRESS", xml_escape(&seller.address_l1)),
            ("CUSTOMER_NIP", xml_escape(customer.nip.as_deref().unwrap_or_default())),
            ("CUSTOMER_NAME", xml_escape(&customer.name)),
            ("COUNTRY_CODE", xml_escape(&customer.country_code)),
            ("CUSTOMER_ADDRESS", xml_escape(&customer.address_l1)),
            ("JST", "2".to_string()),
            ("GV", "2".to_string()),
            ("ISSUE_DATE", self.issue_date.to_string()),
            ("ISSUE_PLACE", String::new()),
            ("INVOICE_NUMBER", xml_escape(&self.number)),
            ("DELIVERY_DATE", self.delivery_date.to_string()),
            ("TOTAL_NET", format!("{:.2}", self.total_net)),
            ("TOTAL_VAT", format!("{:.2}", self.total_vat)),
            ("TOTAL_GROSS", format!("{:.2}", self.total_gross)),
            ("CASH_ACCOUNTING", "2".to_string()),
            ("SELF_BILLING", "2".to_string()),
            ("REVERSE_CHARGE", "2".to_string()),
            ("MANDATORY_SPLIT_PAYMENT", "2".to_string()),
            ("VAT_EXEMPTION_NA", "1".to_string()),
            ("NEW_MEANS_TRANSPORT_NA", "1".to_string()),
            ("TRIANGULAR_TRANSACTION", "2".to_string()),
            ("MARGIN_SCHEME_NA", "1".to_string()),
            ("INVOICE_LINES", invoice_line),
            ("PAYMENT_DEADLINE", self.payment_deadline.to_string()),
            ("PAYMENT_TYPE", DEFAULT_PAYMENT_TYPE.to_string()),
            ("BANK_ACCOUNT", bank_account),
        ];
        fill_placeholders(INVOICE_TEMPLATE, &values)
    }
}

fn round_money(amount: f64) -> f64 {
    (amount * 100.0).round() / 100.0
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn fill_placeholders(template: &str, values: &[(&str, String)]) -> anyhow::Result<String> {
    let mut placeholders: HashMap<&str, Vec<&String>> = HashMap::new();
    for (name, value) in values {
        placeholders.entry(name).or_default().push(value);
    }
    let mut next_occurrence: HashMap<&str, usize> = HashMap::new();
    let mut xml = String::with_capacity(template.len());
    let mut remaining = template;
    while let Some(start) = remaining.find("{{") {
        xml.push_str(&remaining[..start]);
        let name_start = start + 2;
        let name_end = remaining[name_start..].find("}}").ok_or_else(|| anyhow::anyhow!("Unterminated placeholder"))? + name_start;
        let name = &remaining[name_start..name_end];
        let occurrence = next_occurrence.entry(name).or_default();
        let replacement = placeholders.get(name)
            .and_then(|values| values.get(*occurrence))
            .ok_or_else(|| anyhow::anyhow!("No value for placeholder {{{name}}}"))?;
        xml.push_str(replacement);
        *occurrence += 1;
        remaining = &remaining[name_end + 2..];
    }
    xml.push_str(remaining);
    Ok(xml)
}

pub async fn upload_invoice(app_user: &AppUser, invoice: &SalesInvoice, notes: &str) -> anyhow::Result<UploadInvoiceResult> {
    let file = reqwest::multipart::Part::bytes(invoice.xml.as_bytes().to_vec()).file_name("invoice.xml").mime_str("application/xml")?;
    let form = reqwest::multipart::Form::new().part("file", file).text("type", "sales").text("notes", notes.to_string());
    let response = reqwest::Client::new()
        .post(format!("{}/app/invoices", cf_worker_url!()))
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
            body["error"].as_str().unwrap_or("Invoice already exists").to_owned()
        ));
    }
    let response = ensure_success(response, "Invoice uploaded").await?;
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
    let response = reqwest::Client::new()
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
    let body: serde_json::Value = response.json().await?;
    if body["success"].as_bool() != Some(true) {
        anyhow::bail!("KSeF submission failed: {}", body["error"].as_str().unwrap_or("unknown error"));
    }
    Ok(serde_json::from_value(body["result"].clone())?)
}

pub async fn download_receipt(app_user: &AppUser, invoice: &SalesInvoice) -> anyhow::Result<PathBuf> {
    let session_reference_number = invoice.session_reference_number.as_deref().ok_or_else(|| anyhow::anyhow!("This invoice was not submitted to KSeF yet"))?;
    let invoice_reference_number = invoice.invoice_reference_number.as_deref().ok_or_else(|| anyhow::anyhow!("This invoice was not submitted to KSeF yet"))?;
    let mut url = reqwest::Url::parse(&format!("{}/ksef/sales/receipt", cf_worker_url!()))?;
    url.query_pairs_mut()
        .append_pair("sessionReferenceNumber", session_reference_number)
        .append_pair("invoiceReferenceNumber", invoice_reference_number);
    let response = reqwest::Client::new().get(url)
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
    let path = PathBuf::from(format!("{safe_number}-UPO.xml"));
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