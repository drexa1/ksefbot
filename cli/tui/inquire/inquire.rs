use crate::MainMenuAction;
use crate::api::users::{self, AppUser};
use crate::api::{customers, invoices};
use crate::login::google::login_with_google;
use crate::login::microsoft::login_with_microsoft;
use crate::login::phone::login_with_phone;
use crate::login::{AuthUser, LoginMethod};
use crate::tui::inquire::invoices_tui::{prompt_create_invoice, prompt_invoice_dates};
use anyhow::Result;
use chrono::{Datelike, Local};
use crossterm::{
    cursor::MoveTo,
    execute,
    style::Stylize,
    terminal::{Clear, ClearType},
};
use inquire::{Select, Text};
use std::io::{self};
use strum::IntoEnumIterator;

#[path = "invoices.rs"]
pub(crate) mod invoices_tui;

pub async fn login_loop() -> Result<AuthUser> {
    println!();
    let last_used = crate::login::last_used_method();
    loop {
        let choices: Vec<String> = LoginMethod::iter().map(|method| {
            let label = method.to_string();
            if last_used.as_ref() == Some(&method) { format!("{} {label}", "(Last used)".green().bold()) } else { label }
        }).collect();
        let selected = Select::new("Welcome to KSeF Bot. How would you like to log in? ➜🚪", choices.clone()).prompt()?;
        let selected_index = choices.iter().position(|choice| choice == &selected).unwrap();
        let method = LoginMethod::iter().nth(selected_index).unwrap();
        if let Some(logged_user) = crate::login::try_resume_method(&method).await {
            return Ok(logged_user);
        }
        let logged_user = match method {
            LoginMethod::Google => login_with_google().await.map(Some),
            LoginMethod::Microsoft => login_with_microsoft().await.map(Some),
            LoginMethod::Phone => login_with_phone().await,
        };
        match logged_user {
            Ok(Some(user)) => return Ok(user),
            Ok(None) => continue,
            Err(error) => println!("{error:#}"),
        }
    }
}

pub async fn main_loop(logged_user: &mut AppUser) -> Result<()> {
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
            MainMenuAction::Obligations => {
                println!("Obligations here...");
                // VAT declaration (JPK)
                // Income tax
                // Social security (ZUS)
                pause()?;
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

pub fn pause() -> Result<()> {
    println!();
    Text::new("Press [Enter] to go back to the main menu...").prompt()?;
    execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0))?;
    Ok(())
}