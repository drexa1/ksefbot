use crate::MainMenuAction;
use crate::api::users::AppUser;
use crate::api::{customers, invoices, settings};
use crate::login::AuthUser;
use anyhow::Result;
use chrono::{Local};
use crossterm::{
    cursor::MoveTo,
    execute,
    style::Stylize,
    terminal::{Clear, ClearType},
};
use inquire::{Confirm, DateSelect, Select, Text};
use std::io::{self};
use strum::IntoEnumIterator;
use crate::api::invoices::create::{create_invoice, download_receipt, load_invoice_parties, preview_sales_invoice, submit_invoice, upload_invoice, UploadInvoiceResult};

pub async fn login_loop() -> Result<AuthUser> {
    loop {
        let method = Select::new("Welcome to KSeF Bot. How would you like to log in?", crate::login::LoginMethod::iter().collect()).prompt()?;
        let logged_user = match method {
            crate::login::LoginMethod::Google => crate::login::login_with_google().await?,
            crate::login::LoginMethod::Microsoft => crate::login::login_with_microsoft().await?,
            crate::login::LoginMethod::Email => crate::login::login_with_email_loop().await?,
        };
        return Ok(logged_user);
    }
}

pub async fn main_loop(logged_user: &AppUser) -> Result<()> {
    loop {
        match Select::new("What shall we do now?", MainMenuAction::iter().collect()).prompt()? {
            MainMenuAction::CreateSalesInvoice => prompt_create_invoice(logged_user).await?,
            MainMenuAction::ListSalesInvoices => {
                let (from, to) = prompt_invoice_dates()?;
                invoices::list_sales_invoices(logged_user, from, to).await?;
            }
            MainMenuAction::ListPurchaseInvoices => {
                let (from, to) = prompt_invoice_dates()?;
                invoices::list_purchase_invoices(logged_user, from, to).await?;
            }
            MainMenuAction::CreateCustomer => customers::create_customer().await?,
            MainMenuAction::ListCustomers => customers::list_customers().await?,
            MainMenuAction::UserSettings => settings::edit_profile().await?,
            MainMenuAction::Exit => return Ok(()),
        }
        pause()?
    }
}

pub fn prompt_invoice_dates() -> Result<(String, String)> {
    let today = Local::now().date_naive();
    let from = DateSelect::new("From date:").with_starting_date(today).prompt()?;
    let to = DateSelect::new("To date:").with_starting_date(today.max(from)).with_min_date(from).prompt()?;
    Ok((from.format("%Y/%m/%d").to_string(), to.format("%Y/%m/%d").to_string()))
}

pub async fn prompt_create_invoice(app_user: &AppUser) -> Result<()> {
    // Fetch invoice counterparties
    let invoice_parties = load_invoice_parties(app_user).await?;
    let customer = if invoice_parties.customers.len() == 1 {
        // There is only one customer, we select it
        &invoice_parties.customers[0]
    } else {
        // Display customer choice
        let customer_choices: Vec<String> = invoice_parties.customers.iter().enumerate().map(|(index, customer)| {
            format!("{}. {} (NIP: {})", index + 1, customer.name, customer.nip.as_deref().unwrap())
        }).collect();
        let selected: String = Select::new("Select customer", customer_choices).prompt()?;
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
    let mut new_invoice = create_invoice(app_user, &invoice_parties.user_contractor, customer, hours_worked).await?;
    // Upload to cloud backend
    if Confirm::new("Save this invoice in your online vault?").with_default(true).prompt()? {
        let notes = "Invoice generated from CLI client";
        match upload_invoice(app_user, &new_invoice, notes).await? {
            UploadInvoiceResult::Uploaded(invoice_id) => {
                println!("  Invoice '{}' uploaded for user '{}'", invoice_id, app_user.id);
            }
            UploadInvoiceResult::AlreadyExists(message) => {
                println!("  {message}");
            }
        }
    }
    // Save .xml to application folder
    if Confirm::new("Save invoice .xml to application folder?").with_default(true).prompt()? {
        let home = std::env::var_os("USERPROFILE").unwrap();
        let app_folder = std::path::PathBuf::from(home).join(".ksefbot");
        std::fs::create_dir_all(&app_folder)?;
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
            std::fs::write(&path, &new_invoice.xml)?;
            println!("  📂 Invoice .xml saved to {}", path.display().to_string().dark_yellow());
        }
    }
    // Submit to KSeF
    if Confirm::new("Submit this invoice to KSeF?").with_default(false).prompt()? {
        let submission = submit_invoice(app_user, &new_invoice).await?;
        new_invoice.submission = Some(submission);
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
    Ok(())
}

fn pause() -> Result<()> {
    println!();
    Text::new("Press [Enter] to go back to the main menu...").prompt()?;
    execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0))?;
    Ok(())
}
