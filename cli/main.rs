use crate::api::users::get_app_user;
use crate::tui::inquire;
use anyhow::Result;
use strum::{Display, EnumIter};

mod login;
mod tui {
    #[path = "inquire/inquire.rs"]
    pub mod inquire;
}
mod api {
    pub mod client;
    #[path = "users/list.rs"]
    pub mod users;
    #[path = "customers/list.rs"]
    pub mod customers;
    #[path = "invoices/list.rs"]
    pub mod invoices;
}
mod obfstr;
mod onboarding;

#[derive(Clone, Display, EnumIter)]
pub enum MainMenuAction {
    #[strum(to_string = "1. 💵 Create new sales invoice")] CreateSalesInvoice,
    #[strum(to_string = "2. 💵 List sales invoices")] ListSalesInvoices,
    #[strum(to_string = "3. 🛒 List purchase invoices")] ListPurchaseInvoices,
    #[strum(to_string = "4. 🏦 Obligations (VAT/Income tax/Social security)")] Obligations,
    #[strum(to_string = "5. 💼 List customers")] ListCustomers,
    #[strum(to_string = "6. 💼 Create new customer")] CreateCustomer,
    #[strum(to_string = "7. 💼 Edit existing customer")] EditCustomer,
    #[strum(to_string = "8. 🧑‍💻 Edit user settings")] UserSettings,
    #[strum(to_string = "9. 🚪 Exit")] Exit
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    dotenvy::from_filename("cli/.env").ok();
    let mut app_user = loop {
        let logged_user = inquire::login_loop().await?;
        match get_app_user(&logged_user).await? {
            Some(app_user) => break app_user,
            None => if let Err(error) = onboarding::onboard(&logged_user).await {
                println!("{error:#}");
            }
        }
    };
    inquire::main_loop(&mut app_user).await?;
    Ok(())
}