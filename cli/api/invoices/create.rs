use crate::api::customers::{AppContractor, load_contractors};
use crate::api::users::AppUser;
use crate::{cf_client_id, cf_client_secret, cf_worker_url};
use chrono::{Datelike, Duration, Local, NaiveDate, SecondsFormat, Utc};
use std::path::PathBuf;
use xmltree::{Element, EmitterConfig, XMLNode};

const INVOICE_TEMPLATE: &str = include_str!("../../../public/schemas/invoice-template.xml");

const DEFAULT_VAT_RATE: u32 = 23;
const DEFAULT_PAYMENT_TERM_DAYS: i64 = 7;

#[derive(Clone, Copy, Default, strum::Display)]
#[allow(dead_code)]
enum PaymentType {
    #[strum(to_string = "1")] Cash,
    #[strum(to_string = "2")] Card,
    #[strum(to_string = "3")] Voucher,
    #[strum(to_string = "4")] Check,
    #[strum(to_string = "5")] Credit,
    #[default]
    #[strum(to_string = "6")] Transfer,
    #[strum(to_string = "7")] Mobile
}

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
    pub hourly_rate: f64,
    pub total_net: f64,
    pub total_vat: f64,
    pub total_gross: f64
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
        contractor.id != user_contractor.id && contractor.nip.as_deref().is_some_and(|nip| !nip.is_empty())
    }).collect();
    if customers.is_empty() {
        anyhow::bail!("No customers found. Add a customer before creating an invoice.");
    }
    Ok(InvoiceParties { user_contractor, customers })
}

pub fn preview_sales_invoice(app_user: &AppUser, hours_worked: u32) -> anyhow::Result<SalesInvoicePreview> {
    let invoice = InvoiceData::with_defaults(app_user, hours_worked)?;
    Ok(SalesInvoicePreview {
        hourly_rate: invoice.hourly_rate,
        total_net: invoice.total_net,
        total_vat: invoice.total_vat,
        total_gross: invoice.total_gross
    })
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
        year: invoice.issue_date.year(),
        xml,
        submission: None
    })
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
        let vat_unrounded = net_unrounded * f64::from(DEFAULT_VAT_RATE) / 100.0;
        let total_net = round_money(net_unrounded);
        let total_vat = round_money(vat_unrounded);
        let total_gross = round_money(total_net + total_vat);
        let year = today.year();
        let month = today.month();
        let posting_date = NaiveDate::from_ymd_opt(year, month, 1)
            .and_then(|date| date.checked_add_months(chrono::Months::new(1)))
            .and_then(|date| date.pred_opt())
            .unwrap();
        let payment_deadline = posting_date.checked_add_signed(Duration::days(DEFAULT_PAYMENT_TERM_DAYS)).unwrap();
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
        let mut root = Element::parse(INVOICE_TEMPLATE.as_bytes())?;
        let header = root.get_mut_child("Naglowek").unwrap();
        set_text(header, "KodFormularza", "FA");
        set_text(header, "DataWytworzeniaFa", &Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true));
        set_text(header, "SystemInfo", "KSeF Bot");

        let seller_element = root.get_mut_child("Podmiot1").unwrap();
        let seller_identification = seller_element.get_mut_child("DaneIdentyfikacyjne").unwrap();
        set_text(seller_identification, "NIP", seller.nip.as_deref().unwrap_or_default());
        set_text(seller_identification, "Nazwa", &seller.name);
        let seller_address = seller_element.get_mut_child("Adres").unwrap();
        set_text(seller_address, "KodKraju", &seller.country_code);
        set_text(seller_address, "AdresL1", &seller_invoice_address(&seller.address_l1));

        let buyer = root.get_mut_child("Podmiot2").unwrap();
        let buyer_identification = buyer.get_mut_child("DaneIdentyfikacyjne").unwrap();
        set_text(buyer_identification, "NIP", customer.nip.as_deref().unwrap_or_default());
        set_text(buyer_identification, "Nazwa", &customer.name);
        let buyer_address = buyer.get_mut_child("Adres").unwrap();
        set_text(buyer_address, "KodKraju", &customer.country_code);
        set_text(buyer_address, "AdresL1", &customer.address_l1);
        set_text(buyer, "JST", "2");
        set_text(buyer, "GV", "2");

        let fa = root.get_mut_child("Fa").unwrap();
        set_text(fa, "P_1", &self.issue_date.to_string());
        set_text(fa, "P_1M", seller.address_l1.split(',').next().unwrap_or_default().trim());
        set_text(fa, "P_2", &self.number);
        set_text(fa, "P_6", &self.delivery_date.to_string());
        set_text(fa, "P_13_1", &format_amount(self.total_net));
        set_text(fa, "P_14_1", &format_amount(self.total_vat));
        set_text(fa, "P_15", &format_amount(self.total_gross));
        set_text(fa, "RodzajFaktury", "VAT");

        let annotations = fa.get_mut_child("Adnotacje").unwrap();
        set_text(annotations, "P_16", "2");
        set_text(annotations, "P_17", "2");
        set_text(annotations, "P_18", "2");
        set_text(annotations, "P_18A", "2");
        set_text(annotations.get_mut_child("Zwolnienie").unwrap(), "P_19N", "1");
        set_text(annotations.get_mut_child("NoweSrodkiTransportu").unwrap(), "P_22N", "1");
        set_text(annotations, "P_23", "2");
        set_text(annotations.get_mut_child("PMarzy").unwrap(), "P_PMarzyN", "1");

        let invoice_line = invoice_line(self);
        insert_at_placeholder(fa, "{{INVOICE_LINES}}", invoice_line);

        let payment = fa.get_mut_child("Platnosc").unwrap();
        set_text(payment.get_mut_child("TerminPlatnosci").unwrap(), "Termin", &self.payment_deadline.to_string());
        set_text(payment, "FormaPlatnosci", &PaymentType::default().to_string());
        if let Some(account) = bank_account.filter(|account| !account.trim().is_empty()) {
            let mut bank = Element::new("RachunekBankowy");
            bank.children.push(XMLNode::Element(text_element("NrRB", account)));
            insert_at_placeholder(payment, "{{BANK_ACCOUNT}}", bank);
        } else {
            remove_placeholder(payment, "{{BANK_ACCOUNT}}");
        }

        let mut xml = Vec::new();
        root.write_with_config(
            &mut xml,
            EmitterConfig::new().perform_indent(true).write_document_declaration(true)
        )?;
        Ok(String::from_utf8(xml)?)
    }
}

fn set_text(parent: &mut Element, name: &str, value: &str) {
    let element = parent.get_mut_child(name).unwrap();
    element.children.clear();
    element.children.push(XMLNode::Text(value.to_string()));
}

fn text_element(name: &str, value: &str) -> Element {
    let mut element = Element::new(name);
    element.children.push(XMLNode::Text(value.to_string()));
    element
}

fn invoice_line(invoice: &InvoiceData) -> Element {
    let mut line = Element::new("FaWiersz");
    for (name, value) in [
        ("NrWierszaFa", "1".to_string()),
        ("P_7", invoice.item_name.clone()),
        ("P_8A", "szt".to_string()),
        ("P_8B", invoice.hours_worked.to_string()),
        ("P_9A", format_amount(invoice.hourly_rate)),
        ("P_11", format_amount(invoice.total_net)),
        ("P_11Vat", format_amount(invoice.total_vat)),
        ("P_12", DEFAULT_VAT_RATE.to_string()),
    ] {
        line.children.push(XMLNode::Element(text_element(name, &value)));
    }
    line
}

fn insert_at_placeholder(parent: &mut Element, placeholder: &str, element: Element) {
    let index = parent.children.iter().position(|node| {
        matches!(node, XMLNode::Text(text) if text.contains(placeholder))
    }).unwrap();
    parent.children.remove(index);
    parent.children.insert(index, XMLNode::Element(element));
}

fn remove_placeholder(parent: &mut Element, placeholder: &str) {
    let index = parent.children.iter().position(|node| {
        matches!(node, XMLNode::Text(text) if text.contains(placeholder))
    }).unwrap();
    parent.children.remove(index);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invoice_xml_updates_nested_template_fields() {
        let invoice = InvoiceData {
            number: "eFA/2026/09/1".to_string(),
            issue_date: NaiveDate::from_ymd_opt(2026, 9, 30).unwrap(),
            delivery_date: NaiveDate::from_ymd_opt(2026, 9, 30).unwrap(),
            payment_deadline: NaiveDate::from_ymd_opt(2026, 10, 7).unwrap(),
            item_name: "Consulting".to_string(),
            hours_worked: 1,
            hourly_rate: 160.0,
            total_net: 160.0,
            total_vat: 36.8,
            total_gross: 196.8
        };
        let contractor = AppContractor {
            id: "seller".to_string(),
            name: "Seller".to_string(),
            nip: Some("1234567890".to_string()),
            country_code: "PL".to_string(),
            address_l1: "Kraków, 30-638, 15/32".to_string()
        };

        let xml = invoice.to_xml(&contractor, &contractor, None).unwrap();

        assert!(!xml.contains("{{"));
        assert!(xml.contains("<AdresL1>Kraków, 30-638, /</AdresL1>"));
        assert!(xml.contains("<JST>2</JST>"));
        assert!(xml.contains("<GV>2</GV>"));
        assert!(xml.contains("<P_13_1>160</P_13_1>"));
        assert!(xml.contains("<P_14_1>36.8</P_14_1>"));
        assert!(xml.contains("<P_15>196.8</P_15>"));
        assert!(xml.contains("<P_8A>szt</P_8A>"));
        assert!(xml.contains("<P_9A>160</P_9A>"));
        assert!(xml.contains("<P_19N>1</P_19N>"));
        assert!(xml.contains("<P_22N>1</P_22N>"));
        assert!(xml.contains("<P_PMarzyN>1</P_PMarzyN>"));
        assert!(xml.contains("<Termin>2026-10-07</Termin>"));
    }
}

fn round_money(amount: f64) -> f64 {
    (amount * 100.0).round() / 100.0
}

fn format_amount(amount: f64) -> String {
    format!("{amount:.2}").trim_end_matches('0').trim_end_matches('.').to_string()
}

fn seller_invoice_address(address: &str) -> String {
    let mut parts = address.split(", ");
    let town = parts.next().unwrap_or_default();
    let postal_code = parts.next().unwrap_or_default();
    let building = parts.next().and_then(|street_and_building| street_and_building.rsplit_once(' ').map(|(_, building)| building)).unwrap_or_default();
    format!("{town}, {postal_code}, {building}/")
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
    let mut url = reqwest::Url::parse(&format!("{}/ksef/sales/receipt", cf_worker_url!()))?;
    url.query_pairs_mut()
        .append_pair("sessionReferenceNumber", &invoice.submission.as_ref().unwrap().session_reference_number)
        .append_pair("invoiceReferenceNumber", &invoice.submission.as_ref().unwrap().invoice_reference_number);
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