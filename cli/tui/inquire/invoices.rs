use std::fs::{create_dir_all, read_dir, read_to_string, write};
use std::io;
use std::path::{Path, PathBuf};
use crate::api::users::AppUser;
use crate::api::customers::AppContractor;
use anyhow::Result;
use chrono::{Datelike, Duration, Local, NaiveDate};
use crossterm::style::Stylize;
use inquire::{Confirm, CustomType, DateSelect, MultiSelect, Select, Text, validator::Validation};
use serde::{Deserialize, Serialize};
use crate::api::invoices::create::{
    create_invoice,
    load_invoice_parties,
    preview_sales_invoice,
    upload_invoice,
    submit_invoice,
    download_receipt,
    UploadInvoiceResult,
    SalesInvoice
};

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

/// ------------------------------------------------------------------------------------------------
/// List invoices
/// ------------------------------------------------------------------------------------------------

pub fn prompt_invoice_dates(today: NaiveDate, months: Option<&[bool; 12]>) -> Result<Option<(String, String)>> {
    let (last_month_start, last_month_end) = month_range(today, 1);
    let (prev_month_start, prev_month_end) = month_range(today, 2);
    let specific_dates = "Specific dates (max allowed by KSeF: 100 days)".to_string();
    let last_month_choice = last_month_start.format("%m %B").to_string();
    let prev_month_choice = prev_month_start.format("%m %B").to_string();
    let choices = vec![prev_month_choice.clone(), last_month_choice.clone(), specific_dates.clone(), "Back ↩️".to_string()];
    if let Some(months) = months {
        println!("  📅 {}: {}", today.year(), invoice_months(today, months));
    }
    let selected = Select::new("Invoice date range:", choices).prompt()?;
    if selected == "Back ↩️" {
        Ok(None)
    } else if selected == prev_month_choice {
        Ok(Some((prev_month_start.format("%Y-%m-%d").to_string(), prev_month_end.format("%Y-%m-%d").to_string())))
    } else if selected == last_month_choice {
        Ok(Some((last_month_start.format("%Y-%m-%d").to_string(), last_month_end.format("%Y-%m-%d").to_string())))
    } else {
        let from = DateSelect::new("From date:").with_starting_date(today).prompt()?;
        let to = DateSelect::new("To date:").with_starting_date(today.max(from)).with_min_date(from).prompt()?;
        Ok(Some((from.format("%Y-%m-%d").to_string(), to.format("%Y-%m-%d").to_string())))
    }
}

fn invoice_months(today: NaiveDate, months: &[bool; 12]) -> String {
    months.iter().enumerate().map(|(index, present)| {
        let marker = if *present { "🧾" } else { " " };
        if index == today.month0() as usize {
            format!("{}{}*{}", "[".dark_yellow(), if *present { "🧾" } else { "" }, "]".dark_yellow())
        } else {
            format!("[{marker}]")
        }
    }).collect()
}

fn month_range(today: NaiveDate, months_ago: i32) -> (NaiveDate, NaiveDate) {
    let total_months = today.year() * 12 + today.month0() as i32 - months_ago;
    let (year, month) = (total_months.div_euclid(12), total_months.rem_euclid(12) as u32 + 1);
    let start = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    let next_month_start = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    }.unwrap();
    (start, next_month_start.pred_opt().unwrap())
}

/// ------------------------------------------------------------------------------------------------
/// Create invoice
/// ------------------------------------------------------------------------------------------------

pub async fn prompt_create_invoice(app_user: &AppUser) -> Result<bool> {
    // Import previously generated (but not submitted) .xml
    let generated_files = generated_invoice_files()?;
    let import_invoice = !generated_files.is_empty() && Select::new("Create a new invoice or import a generated XML?", vec![
        "📄 Create a new invoice",
        "📂 Import a generated XML"
    ]).prompt()? == "📂 Import a generated XML";
    let (mut new_invoice, imported_path) = if import_invoice {
        let Some((invoice, path)) = import_generated_invoice(&generated_files)? else {
            return Ok(false);
        };
        (invoice, Some(path))
    } else {
        let mode = Select::new("Create sales invoice:", vec!["Simplified (hours worked this month)", "Custom"]).prompt()?;
        // Fetch invoice counterparties
        let invoice_parties = load_invoice_parties(app_user).await?;
        let customer = if invoice_parties.customers.len() == 1 {
            &invoice_parties.customers[0]
        } else {
            let customer_choices: Vec<String> = invoice_parties.customers.iter().enumerate().map(|(index, customer)| {
                format!("{}. {} (NIP: {})", index + 1, customer.name, customer.nip.as_deref().unwrap_or("-"))
            }).collect();
            let selected: String = Select::new("Select customer", customer_choices).with_page_size(15).prompt()?;
            let customer_index = selected.split_once(". ")
                .map(|(index, _)| index.parse::<usize>().unwrap() - 1)
                .unwrap();
            &invoice_parties.customers[customer_index]
        };
        let input = if mode == "Custom" {
            custom(app_user, &invoice_parties.user_contractor, customer)?
        } else {
            simplified(customer)?
        };
        let new_invoice = create_invoice(app_user, &input).await?;
        let preview = preview_sales_invoice(&new_invoice)?;
        println!("  Invoice {}: {} PLN net + {:.2} PLN VAT = {:.2} PLN gross",
                 new_invoice.invoice_number,
                 format!("{:.2}", preview.total_net).green(),
                 preview.total_vat,
                 format!("{:.2}", preview.total_gross).blue()
        );
        // Upload to cloud backend
        if Confirm::new("Save this invoice in your online vault?").with_default(true).prompt()? {
            let notes = "Invoice generated from CLI client";
            match upload_invoice(app_user, &new_invoice, notes).await? {
                UploadInvoiceResult::Uploaded(invoice_id) => {
                    println!("  Invoice '{}' uploaded for user '{}'", invoice_id, app_user.id);
                }
                UploadInvoiceResult::AlreadyExists(message) => {
                    println!("  ✅ {message}");
                }
            }
        }
        // Save .xml to application folder
        if Confirm::new("Save invoice .xml to application folder?").with_default(true).prompt()? {
            let home = std::env::var_os("USERPROFILE").unwrap();
            let app_folder = PathBuf::from(home).join(".ksefbot").join("generated");
            create_dir_all(&app_folder)?;
            let file_stem = format!("{}-{}", new_invoice.month_name, new_invoice.year);
            let original_path = app_folder.join(format!("{file_stem}.xml"));
            let path = if original_path.exists() {
                let choice = Select::new(
                    "An invoice file for this month already exists. What shall we do?",
                    vec!["1. Overwrite existing file", "2. Save a new file"]
                ).prompt()?;
                match choice {
                    "1. Overwrite existing file" => Some(original_path),
                    "2. Save a new file" => {
                        let mut suffix = 2;
                        let path = loop {
                            let candidate = app_folder.join(format!("{file_stem}-{suffix}.xml"));
                            if !candidate.exists() {
                                break candidate;
                            }
                            suffix += 1;
                        };
                        Some(path)
                    }
                    _ => None,
                }
            } else {
                Some(original_path)
            };
            if let Some(path) = path {
                write(&path, &new_invoice.xml)?;
                println!("  📂 Invoice .xml saved to {}", path.display().to_string().dark_yellow());
            }
        }
        (new_invoice, None)
    };
    // Submit to KSeF
    if Confirm::new("Submit this invoice to KSeF?").with_default(false).prompt()? {
        let submission = submit_invoice(app_user, &new_invoice).await?;
        new_invoice.submission = Some(submission);
        if let Some(path) = imported_path {
            let submitted_path = move_to_submitted(&path)?;
            println!("  📂 Invoice XML moved to {}", submitted_path.display().to_string().dark_yellow());
        }
        println!("  Invoice submitted to KSeF.");
        println!("  Invoice number: {}", new_invoice.invoice_number);
        let submission = new_invoice.submission.as_ref().unwrap();
        println!("  Invoice reference: {}", submission.invoice_reference_number);
        println!("  KSeF session: {}", submission.session_reference_number);
        if Confirm::new("Download the KSeF receipt?").with_default(true).prompt()? {
            let save_path = download_receipt(app_user, &new_invoice).await?;
            println!("  📂 Submission receipt saved to {}", save_path.display().to_string().dark_yellow());
        }
    } else {
        println!("  Invoice generated but not submitted.");
    }
    Ok(true)
}

fn generated_invoice_files() -> Result<Vec<PathBuf>> {
    let home = std::env::var_os("USERPROFILE").unwrap();
    let generated_folder = PathBuf::from(home).join(".ksefbot").join("generated");
    create_dir_all(&generated_folder)?;
    let mut files: Vec<_> = read_dir(&generated_folder)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<io::Result<Vec<_>>>()?;
    files.retain(|path| path.is_file() && path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("xml")));
    files.sort();
    Ok(files)
}

fn import_generated_invoice(files: &[PathBuf]) -> Result<Option<(SalesInvoice, PathBuf)>> {
    let mut choices: Vec<String> = files.iter()
        .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    choices.push("Back ↩️".to_string());
    let selected = Select::new("Select a generated invoice XML", choices).prompt()?;
    if selected == "Back ↩️" {
        return Ok(None);
    }
    let index = files.iter().position(|path| path.file_name().unwrap().to_string_lossy() == selected).unwrap();
    let xml = read_to_string(&files[index])?;
    Ok(Some((SalesInvoice::from_xml(xml)?, files[index].clone())))
}

fn move_to_submitted(source: &Path) -> Result<PathBuf> {
    let home = std::env::var_os("USERPROFILE").unwrap();
    let submitted_folder = PathBuf::from(home).join(".ksefbot").join("submitted");
    create_dir_all(&submitted_folder)?;
    let file_name = source.file_name().unwrap();
    let target = submitted_folder.join(file_name);
    let target = if target.exists() {
        let stem = source.file_stem().unwrap().to_string_lossy();
        let extension = source.extension().unwrap().to_string_lossy();
        let mut suffix = 2;
        loop {
            let candidate = submitted_folder.join(format!("{stem}-{suffix}.{extension}"));
            if !candidate.exists() {
                break candidate;
            }
            suffix += 1;
        }
    } else {
        target
    };
    std::fs::rename(source, &target)?;
    Ok(target)
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
    let today = Local::now().date_naive();
    let month_end = month_range(today, 0).1;
    let invoice_number = text("Invoice number:", &format!("eFA/{}/{:02}/1", today.year(), today.month()))?;
    let issue_date = DateSelect::new("Issue date:").with_starting_date(month_end).prompt()?;
    let issue_place = text("Issue place (optional):", seller.address_l1.split(',').next().unwrap().trim())?;
    let posting_date = today;
    let delivery_date = DateSelect::new("Delivery/Service date:").with_starting_date(month_end).prompt()?;
    let markings = MultiSelect::new("Optional markings:", vec!["MPP", "MK", "FP", "TP"]).prompt()?;
    println!("  Customer: {} - {}", customer.nip.as_deref().unwrap_or("-"), customer.name);
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
            unit_price: number("Unit price (PLN):", app_user.default_hourly_rate.unwrap_or(0.0), 0.0)?,
            quantity: number("Quantity:", 1.0, 1.0)?,
            unit: "szt".to_string(),
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
    let invoice_paid = Confirm::new("Invoice already paid?").with_default(false).prompt()?;
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
        let days = CustomType::<u32>::new("Days to pay:").with_starting_input("7").prompt()?;
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
    Ok(CustomType::<f64>::new(message).with_starting_input(&initial.to_string()).with_validator(move |value: &f64| {
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
