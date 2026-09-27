use crate::{MainMenuAction, customers, invoices, settings};
use anyhow::Result;
use crossterm::{
    cursor::MoveTo,
    execute,
    terminal::{Clear, ClearType},
};
use inquire::{Select, Text};
use std::io::{self};
use strum::IntoEnumIterator;
use crate::login::AuthUser;

pub async fn login_loop() -> Result<AuthUser> {
    loop {
        let method = Select::new("Welcome to KSeF Bot. How would you like to log in?", crate::login::LoginMethod::iter().collect()).prompt()?;
        let logged_user = match method {
            crate::login::LoginMethod::Google => crate::login::login_with_google().await?,
            crate::login::LoginMethod::Microsoft => crate::login::login_with_microsoft().await?,
            crate::login::LoginMethod::Email => crate::login::login_with_email_loop().await?
        };
        println!("Login successful.");
        println!();
        return Ok(logged_user)
    }
}

pub async fn main_loop(logged_user: &AuthUser) -> Result<()> {
    println!("Logged user: {}", logged_user.email);
    loop {
        match Select::new("What shall we do now?", MainMenuAction::iter().collect()).prompt()? {
            MainMenuAction::CreateSalesInvoice => invoices::create_sales_invoice().await?,
            MainMenuAction::ListSalesInvoices => invoices::list_sales_invoices().await?,
            MainMenuAction::ListPurchaseInvoices => invoices::list_purchase_invoices().await?,
            MainMenuAction::CreateContractor => customers::create_customer().await?,
            MainMenuAction::ListContractors => customers::list_customers().await?,
            MainMenuAction::UserSettings => settings::edit_profile().await?,
            MainMenuAction::Exit => return Ok(())
        }
        pause()?
    }
}

fn pause() -> Result<()> {
    println!();
    Text::new("Press [Enter] to go back to the main menu...").prompt()?;
    execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0))?;
    Ok(())
}