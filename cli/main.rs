use crate::api::users::{create::init_app_user, get_app_user};
use crate::tui::inquire;
use anyhow::Result;
use strum::{Display, EnumIter};

mod tui {
    #[path = "inquire/inquire.rs"]
    pub mod inquire;
}
mod login;
mod api {
    #[path = "customers/list.rs"]
    pub mod customers;
    #[path = "invoices/list.rs"]
    pub mod invoices;
    #[path = "users/read.rs"]
    pub mod users;
    pub mod client;
}
mod obfstr;

#[derive(Clone, Display, EnumIter)]
pub enum MainMenuAction {
    #[strum(to_string = "1. 💵 Create new sales invoice")] CreateSalesInvoice,
    #[strum(to_string = "2. 💵 List sales invoices")] ListSalesInvoices,
    #[strum(to_string = "3. 🛒 List purchase invoices")] ListPurchaseInvoices,
    #[strum(to_string = "4. 💼 List customers")] ListCustomers,
    #[strum(to_string = "5. 💼 Create new customer")] CreateCustomer,
    #[strum(to_string = "6. 💼 Edit existing customer")] EditCustomer,
    #[strum(to_string = "7. 🧑‍💻 Edit user settings")] UserSettings,
    #[strum(to_string = "8. 🚪 Exit")] Exit
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    dotenvy::from_filename("cli/.env").ok();
    let logged_user = inquire::login_loop().await?;
    let app_user = match get_app_user(&logged_user).await? {
        Some(app_user) => app_user,
        None => init_app_user(&logged_user).await?,
    };
    inquire::main_loop(&app_user).await?;
    Ok(())
}
