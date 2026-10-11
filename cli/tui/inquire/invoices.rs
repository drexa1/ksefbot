use crate::api::customers::AppContractor;
use crate::api::users::AppUser;
use anyhow::Result;
use chrono::{Datelike, Duration, Utc};
use inquire::{Confirm, CustomType, DateSelect, MultiSelect, Select, Text, validator::Validation};
use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceInput {
    pub customer_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invoice_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hours: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer: Option<AppContractor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_entity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue_place: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub posting_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivery_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marking_mpp: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marking_mk: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marking_fp: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marking_tp: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<InvoiceItem>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bank_account: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invoice_paid: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_days: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_deadline: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_term: Option<PaymentTerm>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_link: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ksef_payment_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub footers: Option<Vec<String>>
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceItem {
    pub name: String,
    pub quantity: f64,
    pub unit: String,
    pub unit_price: f64,
    pub vat_rate: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gtu: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub procedure: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_information: Option<String>
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentTerm {
    pub period_length: f64,
    pub period_unit: String,
    pub starting_event: String
}

pub fn simplified(customer: &AppContractor) -> Result<InvoiceInput> {
    let hours = loop {
        let value = Text::new("Hours worked this month:").with_placeholder("160").prompt()?;
        match value.trim().parse::<u32>() {
            Ok(hours) if hours > 0 => break hours,
            _ => println!("The number of hours must be greater than zero."),
        }
    };
    Ok(InvoiceInput { customer_id: customer.id.clone(), hours: Some(hours), ..Default::default() })
}

pub fn custom(app_user: &AppUser, seller: &AppContractor, customer: &AppContractor) -> Result<InvoiceInput> {
    let today = Utc::now().date_naive();
    let invoice_number = text("Invoice number:", &format!("eFA/{}/{:02}/1", today.year(), today.month()))?;
    let issue_date = DateSelect::new("Issue date:").with_starting_date(today).prompt()?;
    let issue_place = text("Issue place (optional):", seller.address_l1.split(',').next().unwrap().trim())?;
    let posting_date = DateSelect::new("Posting date:").with_starting_date(super::month_range(today, 0).1).prompt()?;
    let delivery_date = DateSelect::new("Delivery / service date:").with_starting_date(posting_date).prompt()?;
    let markings = MultiSelect::new("Optional markings:", vec!["MPP", "MK", "FP", "TP"]).prompt()?;
    println!("  Customer: {} ({})", customer.name, customer.id);
    let customer_email = text("Customer email (optional):", "")?;
    let additional_entity = choice("Additional entity:", &[
        ("None", "none"), ("Local government unit (JST)", "jst"), ("VAT group (GV)", "gv"), ("Other", "other")
    ], 0)?;
    let price_type = choice("Unit prices:", &[("Net", "net"), ("Gross", "gross")], 0)?;
    let columns = MultiSelect::new("Additional item fields:", vec!["GTU", "Procedure", "Additional information"]).prompt()?;
    let mut items = Vec::new();
    loop {
        println!("  Item {}", items.len() + 1);
        items.push(InvoiceItem {
            name: text("Item name:", app_user.default_item_name.as_deref().unwrap_or(""))?,
            unit_price: number("Unit price:", app_user.default_hourly_rate.unwrap_or(0.0), 0.0)?,
            quantity: number("Quantity:", 1.0, 1.0)?,
            unit: text("Unit:", "szt")?,
            vat_rate: choice("VAT rate:", &[("23%", "23"), ("8%", "8"), ("5%", "5"), ("0%", "0"), ("Exempt (ZW)", "ZW")], 0)?,
            gtu: if columns.contains(&"GTU") { Some(text("GTU:", "")?) } else { None },
            procedure: if columns.contains(&"Procedure") { Some(text("Procedure symbol:", "")?) } else { None },
            additional_information: if columns.contains(&"Additional information") { Some(text("Additional information:", "")?) } else { None }
        });
        if !Confirm::new("Add another item?").with_default(false).prompt()? { break; }
    }
    let payment_type = choice("Payment type:", &[
        ("Cash", "1"), ("Card", "2"), ("Voucher", "3"), ("Check", "4"),
        ("Loan", "5"), ("Transfer", "6"), ("Mobile payment", "7"), ("Other payment", "*")
    ], 5)?;
    let bank_account = text("Bank account (optional):", app_user.bank_account_number.as_deref().unwrap_or(""))?;
    let invoice_paid = Confirm::new("Invoice paid?").with_default(false).prompt()?;
    let mut input = InvoiceInput {
        customer_id: customer.id.clone(),
        invoice_number: Some(invoice_number),
        issue_date: Some(issue_date.to_string()),
        issue_place: Some(issue_place),
        posting_date: Some(posting_date.to_string()),
        delivery_date: Some(delivery_date.to_string()),
        marking_mpp: Some(markings.contains(&"MPP")),
        marking_mk: Some(markings.contains(&"MK")),
        marking_fp: Some(markings.contains(&"FP")),
        marking_tp: Some(markings.contains(&"TP")),
        customer_email: Some(customer_email),
        additional_entity: Some(additional_entity),
        price_type: Some(price_type),
        items: Some(items),
        payment_type: Some(payment_type),
        bank_account: Some(bank_account),
        invoice_paid: Some(invoice_paid),
        ..Default::default()
    };
    if Select::new("Payment term:", vec!["Deadline", "Description"]).prompt()? == "Deadline" {
        let days = CustomType::<u32>::new("Days to pay:").with_default(7).prompt()?;
        let deadline = DateSelect::new("Payment deadline:")
            .with_starting_date(posting_date + Duration::days(i64::from(days))).prompt()?;
        input.payment_days = Some(days);
        input.payment_deadline = Some(deadline.to_string());
    } else {
        input.payment_term = Some(PaymentTerm {
            period_length: number("Length of period:", 7.0, 0.0)?,
            period_unit: text("Unit of time:", "days")?,
            starting_event: text("Starting event:", "")?
        });
    }
    input.payment_link = Some(text("Payment link (optional):", "")?);
    input.ksef_payment_id = Some(text("KSeF payment ID (optional):", "")?);
    let mut footers = Vec::new();
    loop {
        let footer = text("Invoice footer (optional):", "")?;
        if !footer.is_empty() { footers.push(footer); }
        if !Confirm::new("Add another footer?").with_default(false).prompt()? { break; }
    }
    input.footers = Some(footers);
    Ok(input)
}

fn text(message: &str, initial: &str) -> Result<String> {
    Ok(Text::new(message).with_initial_value(initial).prompt()?.trim().to_string())
}

fn number(message: &str, initial: f64, minimum: f64) -> Result<f64> {
    Ok(CustomType::<f64>::new(message).with_default(initial).with_validator(move |value: &f64| {
        Ok(if value.is_finite() && *value >= minimum {
            Validation::Valid
        } else {
            Validation::Invalid(format!("Enter a number of at least {minimum}.").into())
        })
    }).prompt()?)
}

fn choice(message: &str, choices: &[(&str, &str)], initial: usize) -> Result<String> {
    let selected = Select::new(message, choices.iter().map(|(label, _)| *label).collect())
        .with_starting_cursor(initial).raw_prompt()?;
    Ok(choices[selected.index].1.to_string())
}