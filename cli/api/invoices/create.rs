use crate::api::customers::{AppContractor, load_contractors};
use crate::api::users::AppUser;
use crate::{cf_client_id, cf_client_secret, cf_worker_url};
use anyhow::Context;
use chrono::{Datelike, Duration, Local, NaiveDate, SecondsFormat, Utc};
use std::fmt::Write as _;
use std::path::PathBuf;

const DEFAULT_VAT_RATE: f64 = 0.23;
const DEFAULT_PAYMENT_TYPE: &str = "6";
const DEFAULT_PAYMENT_TERM_DAYS: i64 = 7;

pub struct SalesInvoiceParties {
    pub seller: AppContractor,
    pub customers: Vec<AppContractor>
}

pub struct CreatedSalesInvoice {
    pub invoice_number: String,
    pub session_reference_number: String,
    pub invoice_reference_number: String
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

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubmissionReferences {
    session_reference_number: String,
    invoice_reference_number: String
}

pub async fn load_invoice_parties(app_user: &AppUser) -> anyhow::Result<SalesInvoiceParties> {
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
        contractor.id != seller.id
            && contractor.nip.as_deref().is_some_and(|nip| !nip.is_empty())
    }).collect();
    Ok(SalesInvoiceParties { seller, customers })
}

pub async fn create_invoice(app_user: &AppUser, seller: &AppContractor, customer: &AppContractor, hours_worked: u32) -> anyhow::Result<CreatedSalesInvoice> {
    if hours_worked <= 0 {
        anyhow::bail!("Hours worked must be greater than zero");
    }
    let api_key = app_user.api_key.as_deref().ok_or_else(|| anyhow::anyhow!("The application user has no API key configured"))?;
    let invoice = InvoiceData::with_defaults(app_user, hours_worked)?;
    let xml = invoice.to_xml(seller, customer, app_user.bank_account_number.as_deref());
    let submitted = submit_invoice(api_key, &app_user.id, &xml).await?;
    let notes = format!("{} invoice for {}", app_user.id, invoice.issue_date.format("%B %Y"));
    save_invoice(api_key, &app_user.id, &xml, &notes).await.context(format!(
        "KSeF accepted the invoice as {}, but it could not be saved",
        submitted.invoice_reference_number
    ))?;
    Ok(CreatedSalesInvoice {
        invoice_number: invoice.number,
        session_reference_number: submitted.session_reference_number,
        invoice_reference_number: submitted.invoice_reference_number
    })
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

    fn to_xml(&self, seller: &AppContractor, customer: &AppContractor, bank_account: Option<&str>) -> String {
        let mut xml = String::new();
        let seller_nip = seller.nip.as_deref().unwrap_or_default();
        let customer_nip = customer.nip.as_deref().unwrap_or_default();
        let _ = write!(xml,
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
             <Faktura xmlns=\"http://crd.gov.pl/wzor/2025/06/25/13775/\">\n\
             <Naglowek><KodFormularza kodSystemowy=\"FA (3)\" wersjaSchemy=\"1-0E\">FA</KodFormularza>\
             <WariantFormularza>3</WariantFormularza><DataWytworzeniaFa>{}</DataWytworzeniaFa>\
             <SystemInfo>e-mikrofirma</SystemInfo></Naglowek>\
             <Podmiot1><DaneIdentyfikacyjne><NIP>{}</NIP><Nazwa>{}</Nazwa></DaneIdentyfikacyjne>\
             <Adres><KodKraju>{}</KodKraju><AdresL1>{}</AdresL1></Adres></Podmiot1>\
             <Podmiot2><DaneIdentyfikacyjne><NIP>{}</NIP><Nazwa>{}</Nazwa></DaneIdentyfikacyjne>\
             <Adres><KodKraju>{}</KodKraju><AdresL1>{}</AdresL1></Adres><JST>2</JST><GV>2</GV></Podmiot2>\
             <Fa><KodWaluty>PLN</KodWaluty><P_1>{}</P_1><P_1M></P_1M><P_2>{}</P_2><P_6>{}</P_6>\
             <P_13_1>{:.2}</P_13_1><P_14_1>{:.2}</P_14_1><P_15>{:.2}</P_15>\
             <Adnotacje><P_16>2</P_16><P_17>2</P_17><P_18>2</P_18><P_18A>2</P_18A>\
             <Zwolnienie><P_19N>1</P_19N></Zwolnienie>\
             <NoweSrodkiTransportu><P_22N>1</P_22N></NoweSrodkiTransportu>\
             <P_23>2</P_23><PMarzy><P_PMarzyN>1</P_PMarzyN></PMarzy></Adnotacje>\
             <RodzajFaktury>VAT</RodzajFaktury>\
             <FaWiersz><NrWierszaFa>1</NrWierszaFa><P_7>{}</P_7><P_8A>hour</P_8A>\
             <P_8B>{}</P_8B><P_9A>{:.2}</P_9A><P_11>{:.2}</P_11><P_11Vat>{:.2}</P_11Vat>\
             <P_12>23</P_12></FaWiersz>\
             <Platnosc><TerminPlatnosci><Termin>{}</Termin></TerminPlatnosci>\
             <FormaPlatnosci>{}</FormaPlatnosci>",
            Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            xml_escape(seller_nip),
            xml_escape(&seller.name),
            xml_escape(&seller.country_code),
            xml_escape(&seller.address_l1),
            xml_escape(customer_nip),
            xml_escape(&customer.name),
            xml_escape(&customer.country_code),
            xml_escape(&customer.address_l1),
            self.issue_date,
            xml_escape(&self.number),
            self.delivery_date,
            self.total_net,
            self.total_vat,
            self.total_gross,
            xml_escape(&self.item_name),
            self.hours_worked,
            self.hourly_rate,
            self.total_net,
            self.total_vat,
            self.payment_deadline,
            DEFAULT_PAYMENT_TYPE
        );
        if let Some(bank_account) = bank_account.filter(|account| !account.trim().is_empty()) {
            let _ = write!(xml, "<RachunekBankowy><NrRB>{}</NrRB></RachunekBankowy>", xml_escape(bank_account));
        }
        xml.push_str("</Platnosc></Fa></Faktura>");
        xml
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

async fn submit_invoice(api_key: &str, user_id: &str, xml: &str) -> anyhow::Result<SubmissionReferences> {
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(xml.as_bytes().to_vec()).file_name("invoice.xml").mime_str("application/xml")?
    );
    let response = reqwest::Client::new()
        .post(format!("{}/ksef/sales", cf_worker_url!()))
        .header("CF-Access-Client-Id", cf_client_id!())
        .header("CF-Access-Client-Secret", cf_client_secret!())
        .header("X-API-Key", api_key)
        .header("X-User-Id", user_id)
        .header("Accept", "application/json")
        .multipart(form)
        .send()
        .await?;
    let response = ensure_success(response, "KSeF submission").await?;
    let body: serde_json::Value = response.json().await?;
    if body["success"].as_bool() != Some(true) {
        anyhow::bail!("KSeF submission failed: {}",body["error"].as_str().unwrap_or("unknown error"));
    }
    Ok(serde_json::from_value(body["result"].clone())?)
}

async fn save_invoice(api_key: &str, user_id: &str, xml: &str, notes: &str) -> anyhow::Result<()> {
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(xml.as_bytes().to_vec()).file_name("invoice.xml").mime_str("application/xml")?
        )
        .text("type", "sales")
        .text("notes", notes.to_string());
    let response = reqwest::Client::new()
        .post(format!("{}/app/invoices", cf_worker_url!()))
        .header("CF-Access-Client-Id", cf_client_id!())
        .header("CF-Access-Client-Secret", cf_client_secret!())
        .header("X-API-Key", api_key)
        .header("X-User-Id", user_id)
        .header("Accept", "application/json")
        .multipart(form)
        .send()
        .await?;
    let response = ensure_success(response, "Invoice archive save").await?;
    let body: serde_json::Value = response.json().await?;
    if body["success"].as_bool() != Some(true) {
        anyhow::bail!("Invoice archive save failed: {}",body["error"].as_str().unwrap_or("unknown error"));
    }
    Ok(())
}

pub async fn download_receipt(app_user: &AppUser, invoice: &CreatedSalesInvoice) -> anyhow::Result<PathBuf> {
    let api_key = app_user.api_key.as_deref().ok_or_else(|| anyhow::anyhow!("The application user has no API key configured"))?;
    let mut url = reqwest::Url::parse(&format!("{}/ksef/sales/receipt", cf_worker_url!()))?;
    url.query_pairs_mut()
        .append_pair("sessionReferenceNumber", &invoice.session_reference_number)
        .append_pair("invoiceReferenceNumber", &invoice.invoice_reference_number);
    let response = reqwest::Client::new().get(url)
        .header("CF-Access-Client-Id", cf_client_id!())
        .header("CF-Access-Client-Secret", cf_client_secret!())
        .header("X-API-Key", api_key)
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