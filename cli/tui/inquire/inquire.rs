use crate::MainMenuAction;
use crate::api::users::{self, AppUser};
use crate::api::{customers, invoices};
use crate::login::AuthUser;
use crate::tui::flows;
use crate::tui::prompt::Prompter;
use anyhow::Result;
use chrono::{Datelike, Local, NaiveDate};
use crossterm::{
    cursor::MoveTo,
    execute,
    style::Stylize,
    terminal::{Clear, ClearType},
};
use inquire::{Confirm, DateSelect, Select, Text};
use std::io::{self};
use strum::IntoEnumIterator;

pub struct InquirePrompter;

impl Prompter for InquirePrompter {
    fn info(&mut self, message: &str) -> Result<()> {
        println!("  {message}");
        Ok(())
    }

    fn select(&mut self, title: &str, choices: &[String]) -> Result<String> {
        Ok(Select::new(title, choices.to_vec()).with_page_size(choices.len().min(15)).prompt()?)
    }

    fn text(&mut self, title: &str, initial: &str) -> Result<String> {
        let mut prompt = Text::new(title);
        if !initial.is_empty() {
            prompt = prompt.with_initial_value(initial);
        }
        Ok(prompt.prompt()?)
    }

    fn confirm(&mut self, title: &str, default: bool) -> Result<bool> {
        Ok(Confirm::new(title).with_default(default).prompt()?)
    }

    fn pause(&mut self) -> Result<()> {
        pause()
    }
}

pub async fn login_loop() -> Result<AuthUser> {
    let last_used = crate::login::last_used_method();
    loop {
        let choices: Vec<String> = crate::login::LoginMethod::iter().map(|method| {
            let label = method.to_string();
            if last_used.as_ref() == Some(&method) { format!("{} {label}", "(last used)".green().bold()) } else { label }
        }).collect();
        let selected = Select::new("Welcome to KSeF Bot. How would you like to log in? âžœðŸšª", choices.clone()).prompt()?;
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
    let mut prompter = InquirePrompter;
    loop {
        match Select::new("What shall we do now?:", menu_actions.clone()).with_page_size(page_size).prompt()? {
            MainMenuAction::CreateSalesInvoice => {
                flows::prompt_create_invoice(logged_user, &mut prompter).await?;
            }
            MainMenuAction::ListSalesInvoices => {
                let (from, to) = prompt_invoice_dates()?;
                invoices::list_sales_invoices(logged_user, from, to, &mut prompter).await?;
            }
            MainMenuAction::ListPurchaseInvoices => {
                let (from, to) = prompt_invoice_dates()?;
                invoices::list_purchase_invoices(logged_user, from, to, &mut prompter).await?;
            }
            MainMenuAction::CreateCustomer => {
                customers::create::create_customer(logged_user, &mut prompter).await?;
            }
            MainMenuAction::EditCustomer => {
                customers::edit::edit_customer(logged_user, &mut prompter).await?;
            }
            MainMenuAction::ListCustomers => {
                customers::list_customers(logged_user, &mut prompter).await?;
            }
            MainMenuAction::UserSettings => {
                users::update::edit_profile(logged_user, &mut prompter).await?;
            }
            MainMenuAction::Exit => return Ok(())
        }
    }
}

pub fn prompt_invoice_dates() -> Result<(String, String)> {
    let today = Local::now().date_naive();
    let (last_month_start, last_month_end) = month_range(today, 1);
    let (prev_month_start, prev_month_end) = month_range(today, 2);
    let specific_dates = "Specific dates (max. allowed by KSeF: 3 months span)".to_string();
    let last_month_choice = last_month_start.format("%m %B").to_string();
    let prev_month_choice = prev_month_start.format("%m %B").to_string();
    let choices = vec![prev_month_choice.clone(), last_month_choice.clone(), specific_dates.clone()];
    let selected = Select::new("Invoice date range:", choices).prompt()?;
    if selected == prev_month_choice {
        Ok((prev_month_start.format("%Y/%m/%d").to_string(), prev_month_end.format("%Y/%m/%d").to_string()))
    } else if selected == last_month_choice {
        Ok((last_month_start.format("%Y/%m/%d").to_string(), last_month_end.format("%Y/%m/%d").to_string()))
    } else {
        let from = DateSelect::new("From date:").with_starting_date(today).prompt()?;
        let to = DateSelect::new("To date:").with_starting_date(today.max(from)).with_min_date(from).prompt()?;
        Ok((from.format("%Y/%m/%d").to_string(), to.format("%Y/%m/%d").to_string()))
    }
}

fn month_range(today: NaiveDate, months_ago: i32) -> (NaiveDate, NaiveDate) {
    let total_months = today.year() * 12 + today.month0() as i32 - months_ago;
    let (year, month) = (total_months.div_euclid(12), total_months.rem_euclid(12) as u32 + 1);
    let start = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    let next_month_start = if month == 12 { NaiveDate::from_ymd_opt(year + 1, 1, 1) } else { NaiveDate::from_ymd_opt(year, month + 1, 1) }.unwrap();
    (start, next_month_start.pred_opt().unwrap())
}

pub fn pause() -> Result<()> {
    println!();
    Text::new("Press [Enter] to go back to the main menu...").prompt()?;
    execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0))?;
    Ok(())
}
