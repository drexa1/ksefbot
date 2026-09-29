use crate::MainMenuAction;
use crate::api::users::AppUser;
use crate::api::{customers, invoices, settings};
use crate::login::AuthUser;
use anyhow::Result;
use chrono::{Local, NaiveDate};
use crossterm::{
    cursor::MoveTo,
    execute,
    terminal::{Clear, ClearType},
};
use inquire::{Confirm, DateSelect, Select, Text};
use std::io::{self};
use strum::IntoEnumIterator;
use crate::api::invoices::create::load_invoice_parties;

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
    let parties = load_invoice_parties(app_user).await?;
    if parties.customers.is_empty() {
        anyhow::bail!("No customers are available. Add a customer before creating an invoice.");
    }
    let customer = if parties.customers.len() == 1 {
        &parties.customers[0]
    } else {
        let customer_choices: Vec<String> = parties.customers.iter().enumerate().map(|(index, customer)| {
            format!("{}. {} (NIP: {})", index + 1, customer.name, customer.nip.as_deref().unwrap())
        }).collect();
        let selected: String = Select::new("Select customer", customer_choices).prompt()?;
        let customer_index = selected.split_once(". ")
            .and_then(|(index, _)| index.parse::<usize>().ok())
            .and_then(|index| index.checked_sub(1))
            .filter(|index| *index < parties.customers.len())
            .ok_or_else(|| anyhow::anyhow!("Selected customer was not found"))?;
        &parties.customers[customer_index]
    };
    let hours_worked = loop {
        let value = Text::new("Hours worked").with_placeholder("e.g. 160").prompt()?;
        match value.trim().parse::<u32>() {
            Ok(hours) if hours > 0 => break hours,
            _ => println!("Enter a whole number of hours greater than zero."),
        }
    };
    let preview = invoices::create::preview_sales_invoice(app_user, hours_worked)?;
    println!(
        "Invoice total will be {:.2} PLN gross ({:.2} PLN net + {:.2} PLN VAT; {} hours at {:.2} PLN/hour).",
        preview.total_gross,
        preview.total_net,
        preview.total_vat,
        hours_worked,
        preview.hourly_rate
    );
    if !Confirm::new("Submit this invoice to KSeF?").with_default(false).prompt()? {
        println!("Invoice submission cancelled.");
        return Ok(());
    }
    let created = invoices::create::create_invoice(app_user, &parties.seller, customer, hours_worked).await?;
    println!("Invoice submitted and saved to the app invoice archive.");
    println!("  Invoice number: {}", created.invoice_number);
    println!("  Invoice reference: {}", created.invoice_reference_number);
    println!("  KSeF session: {}", created.session_reference_number);
    if Confirm::new("Download the KSeF receipt now?").with_default(false).prompt()? {
        let path = invoices::create::download_receipt(app_user, &created).await?;
        println!("Receipt saved to {}", path.display());
    }
    Ok(())
}

fn pause() -> Result<()> {
    println!();
    Text::new("Press [Enter] to go back to the main menu...").prompt()?;
    execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0))?;
    Ok(())
}
