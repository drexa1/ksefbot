use crate::MainMenuAction;
use crate::api::users::AppUser;
use crate::api::{customers, invoices, settings};
use crate::login::AuthUser;
use anyhow::Result;
use chrono::{Local};
use crossterm::{
    cursor::MoveTo,
    execute,
    terminal::{Clear, ClearType},
};
use inquire::{Confirm, DateSelect, Select, Text};
use std::io::{self};
use strum::IntoEnumIterator;
use crate::api::invoices::create::{create_invoice, download_receipt, load_invoice_parties, submit_invoice};

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
    let invoice_parties = load_invoice_parties(app_user).await?;
    if invoice_parties.customers.is_empty() {
        anyhow::bail!("No customers found. Add a customer before creating an invoice.");
    }
    let customer = if invoice_parties.customers.len() == 1 {
        &invoice_parties.customers[0]
    } else {
        let customer_choices: Vec<String> = invoice_parties.customers.iter().enumerate().map(|(index, customer)| {
            format!("{}. {} (NIP: {})", index + 1, customer.name, customer.nip.as_deref().unwrap())
        }).collect();
        let selected: String = Select::new("Select customer", customer_choices).prompt()?;
        let customer_index = selected.split_once(". ")
            .and_then(|(index, _)| index.parse::<usize>().ok())
            .and_then(|index| index.checked_sub(1))
            .filter(|index| *index < invoice_parties.customers.len())
            .ok_or_else(|| anyhow::anyhow!("Customer not found"))?;
        &invoice_parties.customers[customer_index]
    };
    let hours_worked = loop {
        let value = Text::new("Hours worked: ").with_placeholder("160").prompt()?;
        match value.trim().parse::<u32>() {
            Ok(hours) if hours > 0 => break hours,
            _ => println!("The number of hours must be greater than zero."),
        }
    };
    let preview = invoices::create::preview_sales_invoice(app_user, hours_worked)?;
    println!(
        "Invoice for [{} hours at {:.2} PLN/hour]: {:.2} PLN net + {:.2} PLN VAT = {:.2} PLN gross",
        hours_worked,
        preview.hourly_rate,
        preview.total_net,
        preview.total_vat,
        preview.total_gross
    );
    let mut new_invoice = create_invoice(app_user, &invoice_parties.seller, customer, hours_worked).await?;
    if Confirm::new("Save this invoice in the app archive?").with_default(false).prompt()? {
        let notes = format!("Invoice {} saved for user {}", new_invoice.invoice_number, app_user.id);
        invoices::create::save_invoice(app_user, &new_invoice, &notes).await?;
        println!("{notes}");
    }
    if Confirm::new("Submit this invoice to KSeF?").with_default(false).prompt()? {
        let submitted = submit_invoice(app_user, &new_invoice).await?;
        new_invoice.session_reference_number = Some(submitted.session_reference_number);
        new_invoice.invoice_reference_number = Some(submitted.invoice_reference_number);
        println!("Invoice submitted to KSeF.");
        println!("  Invoice number: {}", new_invoice.invoice_number);
        println!("  Invoice reference: {}", new_invoice.invoice_reference_number.as_deref().unwrap_or_default());
        println!("  KSeF session: {}", new_invoice.session_reference_number.as_deref().unwrap_or_default());
        if Confirm::new("Download the KSeF receipt?").with_default(true).prompt()? {
            let save_path = download_receipt(app_user, &new_invoice).await?;
            println!("Receipt saved at {}", save_path.display());
        }
    } else {
        println!("Invoice created locally but not submitted to KSeF.");
    }
    Ok(())
}

fn pause() -> Result<()> {
    println!();
    Text::new("Press [Enter] to go back to the main menu...").prompt()?;
    execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0))?;
    Ok(())
}
