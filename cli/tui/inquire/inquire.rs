use crate::MainMenuAction;
use crate::api::invoices::create::{SalesInvoice, UploadInvoiceResult, create_invoice, download_receipt, load_invoice_parties, preview_sales_invoice, submit_invoice, upload_invoice};
use crate::api::users::{self, AppUser};
use crate::api::{customers, invoices};
use crate::login::AuthUser;
use anyhow::Result;
use chrono::{Datelike, Local, NaiveDate};
use crossterm::{
    cursor::MoveTo,
    execute,
    style::Stylize,
    terminal::{Clear, ClearType},
};
use inquire::{Confirm, DateSelect, Select, Text};
use std::fs::{create_dir_all, read_dir, read_to_string, write};
use std::io::{self};
use std::path::{Path, PathBuf};
use strum::IntoEnumIterator;

pub async fn login_loop() -> Result<AuthUser> {
    let last_used = crate::login::last_used_method();
    loop {
        let choices: Vec<String> = crate::login::LoginMethod::iter().map(|method| {
            let label = method.to_string();
            if last_used.as_ref() == Some(&method) { format!("{} {label}", "(Last used)".green().bold()) } else { label }
        }).collect();
        let selected = Select::new("Welcome to KSeF Bot. How would you like to log in? ➜🚪", choices.clone()).prompt()?;
        let selected_index = choices.iter().position(|choice| choice == &selected).unwrap();
        let method = crate::login::LoginMethod::iter().nth(selected_index).unwrap();
        if let Some(logged_user) = crate::login::try_resume_method(&method).await {
            return Ok(logged_user);
        }
        let logged_user = match method {
            crate::login::LoginMethod::Google => crate::login::login_with_google().await?,
            crate::login::LoginMethod::Microsoft => crate::login::login_with_microsoft().await?,
            crate::login::LoginMethod::Phone => crate::login::login_with_phone_loop().await?,
        };
        return Ok(logged_user);
    }
}

pub async fn main_loop(logged_user: &AppUser) -> Result<()> {
    println!();
    let menu_actions: Vec<MainMenuAction> = MainMenuAction::iter().collect();
    let page_size = menu_actions.len();
    loop {
        match Select::new("What shall we do now?:", menu_actions.clone()).with_page_size(page_size).prompt()? {
            MainMenuAction::CreateSalesInvoice => {
                if prompt_create_invoice(logged_user).await? {
                    pause()?;
                }
            }
            MainMenuAction::ListSalesInvoices => {
                let today = Local::now().date_naive();
                let months = invoices::sales_invoice_months(logged_user, today.year()).await?;
                let Some((from, to)) = prompt_invoice_dates(today, Some(&months))? else { continue };
                invoices::list_sales_invoices(logged_user, from, to).await?;
            }
            MainMenuAction::ListPurchaseInvoices => {
                let Some((from, to)) = prompt_invoice_dates(Local::now().date_naive(), None)? else { continue };
                invoices::list_purchase_invoices(logged_user, from, to).await?;
            }
            MainMenuAction::CreateCustomer => {
                customers::create::create_customer(logged_user).await?;
                pause()?;
            }
            MainMenuAction::EditCustomer => {
                if customers::edit::edit_customer(logged_user).await? {
                    pause()?;
                }
            }
            MainMenuAction::ListCustomers => {
                customers::list_customers(logged_user).await?;
                pause()?;
            }
            MainMenuAction::UserSettings => {
                if users::update::edit_profile(logged_user).await? {
                    pause()?;
                }
            }
            MainMenuAction::Exit => return Ok(())
        }
    }
}

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
    let next_month_start = if month == 12 { NaiveDate::from_ymd_opt(year + 1, 1, 1) } else { NaiveDate::from_ymd_opt(year, month + 1, 1) }.unwrap();
    (start, next_month_start.pred_opt().unwrap())
}

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
        // Fetch invoice counterparties
        let invoice_parties = load_invoice_parties(app_user).await?;
        let customer = if invoice_parties.customers.len() == 1 {
            &invoice_parties.customers[0]
        } else {
            let customer_choices: Vec<String> = invoice_parties.customers.iter().enumerate().map(|(index, customer)| {
                format!("{}. {} (NIP: {})", index + 1, customer.name, customer.nip.as_deref().unwrap())
            }).collect();
            let selected: String = Select::new("Select customer", customer_choices).with_page_size(15).prompt()?;
            let customer_index = selected.split_once(". ")
                .map(|(index, _)| index.parse::<usize>().unwrap() - 1)
                .unwrap();
            &invoice_parties.customers[customer_index]
        };
        // Prompt hours worked
        let hours_worked = loop {
            let value = Text::new("Hours worked: ").with_placeholder("160").prompt()?;
            match value.trim().parse::<u32>() {
                Ok(hours) if hours > 0 => break hours,
                _ => println!("The number of hours must be greater than zero."),
            }
        };
        // Preview on screen
        let preview = preview_sales_invoice(app_user, hours_worked)?;
        println!("  🔎 Invoice for [{} hours at {:.2} PLN/h]: {} PLN net + {:.2} PLN VAT = {:.2} PLN gross",
            hours_worked,
            preview.hourly_rate,
            format!("{:.2}", preview.total_net).green(),
            preview.total_vat,
            format!("{:.2}", preview.total_gross).blue()
        );
        // Generate .xml
        let new_invoice = create_invoice(app_user, &invoice_parties.user_contractor, customer, hours_worked).await?;
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
        println!("  Invoice created but not submitted.");
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

pub fn pause() -> Result<()> {
    Text::new("Press [Enter] to go back to the main menu...").prompt()?;
    execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0))?;
    Ok(())
}
