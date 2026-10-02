use crate::api::users::AppUser;
use crate::login::AuthUser;
use crate::{MainMenuAction, login};
use anyhow::Result;
use chrono::{Datelike, Local, Months, NaiveDate};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    style::{Modifier, Style},
    widgets::calendar::{CalendarEventStore, Monthly},
};
use std::io;
use strum::IntoEnumIterator;

#[path = "widgets.rs"]
mod widgets;
#[path = "invoices.rs"]
mod invoices;
#[path = "customers.rs"]
mod customers;
#[path = "users.rs"]
mod users;

pub type Tui = Terminal<CrosstermBackend<io::Stdout>>;

pub async fn login_loop(terminal: &mut Tui) -> Result<AuthUser> {
    let methods: Vec<_> = login::LoginMethod::iter().collect();
    let last_used = login::last_used_method();
    let choices: Vec<_> = methods.iter().map(|method| {
        format!("{method}{}", if last_used.as_ref() == Some(method) { " (last used)" } else { "" })
    }).collect();
    let mut selected = methods.iter().position(|method| Some(method) == last_used.as_ref()).unwrap_or_default();
    loop {
        let Some(index) = widgets::select_index(terminal, "Sign in to KSeF Bot", &choices, &mut selected)? else {
            anyhow::bail!("Login cancelled");
        };
        let method = &methods[index];
        terminal.draw(|frame| {
            widgets::modal(frame, "Signing in", 64, 1, "Complete sign-in in your browser if requested.");
        })?;
        if let Some(user) = login::try_resume_method(method).await {
            return Ok(user);
        }
        let user = match method {
            login::LoginMethod::Google => login::login_with_google().await,
            login::LoginMethod::Microsoft => login::login_with_microsoft().await,
            login::LoginMethod::Phone => {
                ratatui::try_restore()?;
                let result = login::login_with_phone_loop().await;
                *terminal = ratatui::try_init()?;
                result
            }
        };
        match user {
            Ok(user) => return Ok(user),
            Err(error) => widgets::message(terminal, "Sign-in failed", &[format!("{error:#}")])?,
        }
    }
}

fn action_label(action: &MainMenuAction) -> &'static str {
    match action {
        MainMenuAction::CreateSalesInvoice => "Create sales invoice",
        MainMenuAction::ListSalesInvoices => "Sales invoices",
        MainMenuAction::ListPurchaseInvoices => "Purchase invoices",
        MainMenuAction::ListCustomers => "Customers",
        MainMenuAction::CreateCustomer => "Create customer",
        MainMenuAction::EditCustomer => "Edit customer",
        MainMenuAction::UserSettings => "User settings",
        MainMenuAction::Exit => "Exit",
    }
}

pub(crate) async fn main_loop(terminal: &mut Tui, app_user: &AppUser) -> Result<()> {
    let actions: Vec<_> = MainMenuAction::iter().collect();
    let choices: Vec<_> = actions.iter().map(|action| action_label(action).to_string()).collect();
    let mut selected = 0;
    loop {
        let Some(index) = widgets::select_index(terminal, "What would you like to do?", &choices, &mut selected)? else { return Ok(()); };
        let result = match &actions[index] {
            MainMenuAction::CreateSalesInvoice => invoices::prompt_create_invoice(terminal, app_user).await,
            MainMenuAction::ListSalesInvoices => {
                let Some((from, to)) = request_invoice_dates(terminal)? else { continue; };
                invoices::browse_sales_invoices(terminal, app_user, from, to).await
            }
            MainMenuAction::ListPurchaseInvoices => {
                let Some((from, to)) = request_invoice_dates(terminal)? else { continue; };
                invoices::browse_purchase_invoices(terminal, app_user, from, to).await
            }
            MainMenuAction::CreateCustomer => customers::create_customer(terminal, app_user).await,
            MainMenuAction::EditCustomer => customers::edit_customer(terminal, app_user).await,
            MainMenuAction::ListCustomers => customers::list_customers(terminal, app_user).await,
            MainMenuAction::UserSettings => users::edit_profile(terminal, app_user).await,
            MainMenuAction::Exit => return Ok(()),
        };
        match result {
            Ok(log) => widgets::message(terminal, action_label(&actions[index]), &log)?,
            Err(error) => widgets::message(terminal, "Action failed", &[format!("{error:#}")])?,
        }
    }
}

fn month_range(today: NaiveDate, months_ago: u32) -> (NaiveDate, NaiveDate) {
    let start = today.with_day(1).unwrap().checked_sub_months(Months::new(months_ago)).unwrap();
    let end = start.checked_add_months(Months::new(1)).unwrap().pred_opt().unwrap();
    (start, end)
}

fn request_invoice_dates(terminal: &mut Tui) -> Result<Option<(String, String)>> {
    let today = Local::now().date_naive();
    let months = [month_range(today, 2), month_range(today, 1)];
    let choices = vec![
        months[0].0.format("%B %Y").to_string(),
        months[1].0.format("%B %Y").to_string(),
        "Specific dates (up to 3 months)".to_string(),
    ];
    let Some(index) = widgets::select_index(terminal, "Invoice date range", &choices, &mut 1)? else { return Ok(None); };
    let (from, to) = if index < months.len() {
        months[index]
    } else {
        let Some(from) = select_date(terminal, "From date", today, None)? else { return Ok(None); };
        let to = loop {
            let Some(to) = select_date(terminal, "To date", today.max(from), Some(from))? else { return Ok(None); };
            if from.checked_add_months(Months::new(3)).is_some_and(|limit| to <= limit) { break to; }
            widgets::message(terminal, "Check date range", &["Choose a range of no more than 3 months.".to_string()])?;
        };
        (from, to)
    };
    Ok(Some((from.format("%Y/%m/%d").to_string(), to.format("%Y/%m/%d").to_string())))
}

fn draw_date(frame: &mut ratatui::Frame, title: &str, selected: NaiveDate, minimum: Option<NaiveDate>) {
    let date = time::Date::from_calendar_date(selected.year(), (selected.month() as u8).try_into().unwrap(), selected.day() as u8).unwrap();
    let title = format!("{title}: {selected}");
    let hint = format!("Arrows: day/week | PgUp/PgDn: month\nTab/Shift+Tab: day | Enter: select | Esc: back{}", minimum.map(|date| format!("\nEarliest: {date}")).unwrap_or_default());
    let Some(area) = widgets::modal(frame, &title, 64, 9, &hint) else { return; };
    let mut events = CalendarEventStore::default();
    events.add(date, widgets::highlight_style());
    let calendar = Monthly::new(date, events)
        .show_month_header(Modifier::BOLD)
        .show_weekdays_header(Modifier::BOLD)
        .show_surrounding(Style::default().add_modifier(Modifier::DIM));
    frame.render_widget(calendar, widgets::centered_rect(area, 22, 9));
}

fn select_date(terminal: &mut Tui, title: &str, initial: NaiveDate, minimum: Option<NaiveDate>) -> Result<Option<NaiveDate>> {
    let mut selected = initial;
    loop {
        terminal.draw(|frame| draw_date(frame, title, selected, minimum))?;
        match read_key()? {
            KeyCode::Enter => return Ok(Some(selected)),
            KeyCode::Esc | KeyCode::Char('q') => return Ok(None),
            key => selected = move_calendar_date(selected, key, minimum),
        }
    }
}

fn move_calendar_date(selected: NaiveDate, key: KeyCode, minimum: Option<NaiveDate>) -> NaiveDate {
    let next = match key {
        KeyCode::Left | KeyCode::BackTab => selected.checked_sub_signed(chrono::Duration::days(1)),
        KeyCode::Right | KeyCode::Tab => selected.checked_add_signed(chrono::Duration::days(1)),
        KeyCode::Up => selected.checked_sub_signed(chrono::Duration::days(7)),
        KeyCode::Down => selected.checked_add_signed(chrono::Duration::days(7)),
        KeyCode::PageUp => selected.checked_sub_months(Months::new(1)),
        KeyCode::PageDown => selected.checked_add_months(Months::new(1)),
        _ => None,
    };
    next.filter(|date| minimum.is_none_or(|min| *date >= min))
        .filter(|date| (time::Date::MIN.year()..=time::Date::MAX.year()).contains(&date.year()))
        .unwrap_or(selected)
}

fn read_key() -> Result<KeyCode> {
    loop {
        match event::read()? {
            Event::Resize(_, _) => return Ok(KeyCode::Null),
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                let code = if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                    KeyCode::Esc
                } else { key.code };
                let (width, height) = crossterm::terminal::size()?;
                return Ok(if (width < 48 || height < 24) && code != KeyCode::Esc { KeyCode::Null } else { code });
            }
            _ => {}
        }
    }
}

pub async fn with_terminal<T>(action: impl AsyncFnOnce(&mut Tui) -> Result<T>) -> Result<T> {
    let mut terminal = ratatui::try_init()?;
    let result = action(&mut terminal).await;
    ratatui::try_restore()?;
    result
}
