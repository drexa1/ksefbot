use anyhow::Result;
use strum::{Display, EnumIter};
use crate::tui::inquire::{login_loop, main_loop};

mod tui {
    pub mod inquire;
    pub mod ratatui;
}

mod login;
mod invoices;
mod customers;
mod settings;
mod obfstr;

#[derive(Clone, Display, EnumIter)]
pub enum MainMenuAction {
    #[strum(to_string = "1. Create new [sales] invoice")] CreateSalesInvoice,
    #[strum(to_string = "2. List [sales] invoices")] ListSalesInvoices,
    #[strum(to_string = "3. List [purchase] invoices")] ListPurchaseInvoices,
    #[strum(to_string = "4. Create new contractor")] CreateContractor,
    #[strum(to_string = "5. List known contractors")] ListContractors,
    #[strum(to_string = "6. User settings")] UserSettings,
    #[strum(to_string = "7. Exit")] Exit
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::from_filename("cli/.env").ok();
    match std::env::var("TUI").as_deref() {
        Ok("inquire") => {
            login_loop().await?;
            main_loop().await?;
        }
        Ok("ratatui") | Err(_) => {
            tui::ratatui::run().await?;
        }
        Ok(tui) => anyhow::bail!("Unknown TUI implementation: {tui}"),
    }
    Ok(())
}