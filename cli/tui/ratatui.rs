use crate::api::users::AppUser;
use crate::api::{customers, invoices, settings};
use crate::login::AuthUser;
use crate::{MainMenuAction, login};
use anyhow::Result;
use chrono::{Datelike, Local, Months, NaiveDate};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::widgets::Paragraph;
use ratatui::widgets::calendar::{CalendarEventStore, Monthly};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState},
};
use std::io;
use strum::IntoEnumIterator;

pub type Tui = Terminal<CrosstermBackend<io::Stdout>>;

pub async fn login_loop(terminal: &mut Tui) -> Result<AuthUser> {
    let last_used = login::last_used_method();
    let methods: Vec<login::LoginMethod> = login::LoginMethod::iter().collect();
    let mut selected = 0usize;
    loop {
        terminal.draw(|frame| draw_login(frame, &methods, selected, last_used.as_ref()))?;
        match read_key()? {
            KeyCode::Up | KeyCode::Char('k') => selected = selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                if selected + 1 < methods.len() {
                    selected += 1;
                }
            }
            KeyCode::Enter => {
                let method = methods[selected].clone();
                if let Some(user) = login::try_resume_method(&method).await {
                    return Ok(user);
                }
                restore_terminal(terminal)?;
                let user = match method {
                    login::LoginMethod::Google => login::login_with_google().await?,
                    login::LoginMethod::Microsoft => login::login_with_microsoft().await?,
                    login::LoginMethod::Email => login::login_with_email_loop().await?,
                };
                setup_terminal_in_place(terminal)?;
                return Ok(user);
            }
            KeyCode::Esc | KeyCode::Char('q') => return Err(anyhow::anyhow!("Login cancelled")),
            _ => {}
        }
    }
}

pub(crate) async fn main_loop(terminal: &mut Tui, app_user: &AppUser) -> Result<()> {
    let actions: Vec<MainMenuAction> = MainMenuAction::iter().collect();
    let mut selected = 0usize;
    loop {
        terminal.draw(|frame| draw_main_menu(frame, &actions, selected))?;
        match read_key()? {
            KeyCode::Up | KeyCode::Char('k') => {
                selected = selected.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if selected + 1 < actions.len() {
                    selected += 1;
                }
            }
            KeyCode::Enter => {
                match actions[selected].clone() {
                    MainMenuAction::CreateSalesInvoice => {
                        restore_terminal(terminal)?;
                        crate::tui::inquire::prompt_create_invoice(app_user).await?;
                    }
                    MainMenuAction::ListSalesInvoices => {
                        let (from, to) = request_invoice_dates(terminal)?;
                        restore_terminal(terminal)?;
                        invoices::list_sales_invoices(app_user, from, to).await?;
                    }
                    MainMenuAction::ListPurchaseInvoices => {
                        let (from, to) = request_invoice_dates(terminal)?;
                        restore_terminal(terminal)?;
                        invoices::list_purchase_invoices(app_user, from, to).await?;
                    }
                    MainMenuAction::CreateCustomer => {
                        restore_terminal(terminal)?;
                        customers::create_customer(app_user).await?;
                    }
                    MainMenuAction::EditCustomer => {
                        restore_terminal(terminal)?;
                        customers::edit_customer(app_user).await?;
                    }
                    MainMenuAction::ListCustomers => {
                        restore_terminal(terminal)?;
                        customers::list_customers(app_user).await?;
                    }
                    MainMenuAction::UserSettings => {
                        restore_terminal(terminal)?;
                        settings::edit_profile().await?;
                    }
                    MainMenuAction::Exit => return Ok(()),
                }
                setup_terminal_in_place(terminal)?;
                pause(terminal)?;
            }
            KeyCode::Esc | KeyCode::Char('q') => return Ok(()),
            _ => {}
        }
    }
}

fn request_invoice_dates(terminal: &mut Tui) -> Result<(String, String)> {
    let today = Local::now().date_naive();
    let from = select_date(terminal, "From date", today, None)?.ok_or_else(|| anyhow::anyhow!("Selection cancelled"))?;
    let to = select_date(terminal, "To date", today.max(from), Some(from))?.ok_or_else(|| anyhow::anyhow!("Selection cancelled"))?;
    Ok((from.format("%Y/%m/%d").to_string(), to.format("%Y/%m/%d").to_string()))
}

fn select_date(terminal: &mut Tui, title: &str, initial: NaiveDate, minimum: Option<NaiveDate>) -> Result<Option<NaiveDate>> {
    let mut selected = initial;
    loop {
        let date = time::Date::from_calendar_date(selected.year(), (selected.month() as u8).try_into()?, selected.day() as u8)?;
        terminal.draw(|frame| {
            let area = centered_rect(frame.area(), 90, 90);
            let sections = Layout::vertical([Constraint::Length(11), Constraint::Min(3)]).split(area);
            let mut events = CalendarEventStore::default();
            events.add(date, Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD));
            let calendar = Monthly::new(date, events)
                .show_month_header(Modifier::BOLD)
                .show_weekdays_header(Modifier::BOLD)
                .show_surrounding(Modifier::DIM)
                .block(Block::default().title(format!("{title}: {selected}")).borders(Borders::ALL));
            frame.render_widget(calendar, sections[0]);
            let minimum_hint = minimum.map(|date| format!("\nEarliest date: {date}")).unwrap_or_default();
            frame.render_widget(Paragraph::new(format!(
                "Arrows: day/week | PgUp/PgDn: month\nEnter: select | Esc/q: cancel{minimum_hint}"
            )), sections[1]);
        })?;
        let key = read_key()?;
        match key {
            KeyCode::Enter => return Ok(Some(selected)),
            KeyCode::Esc | KeyCode::Char('q') => return Ok(None),
            _ => selected = move_calendar_date(selected, key, minimum),
        }
    }
}

fn move_calendar_date(selected: NaiveDate, key: KeyCode, minimum: Option<NaiveDate>) -> NaiveDate {
    let next = match key {
        KeyCode::Left => selected.checked_sub_signed(chrono::Duration::days(1)),
        KeyCode::Right => selected.checked_add_signed(chrono::Duration::days(1)),
        KeyCode::Up => selected.checked_sub_signed(chrono::Duration::days(7)),
        KeyCode::Down => selected.checked_add_signed(chrono::Duration::days(7)),
        KeyCode::PageUp => selected.checked_sub_months(Months::new(1)),
        KeyCode::PageDown => selected.checked_add_months(Months::new(1)),
        _ => None,
    };
    next
        .filter(|date| minimum.is_none_or(|min| *date >= min))
        .filter(|date| (time::Date::MIN.year()..=time::Date::MAX.year()).contains(&date.year()))
        .unwrap_or(selected)
}

fn pause(terminal: &mut Tui) -> Result<()> {
    loop {
        terminal.draw(draw_pause)?;
        match read_key()? {
            KeyCode::Enter => return Ok(()),
            KeyCode::Esc | KeyCode::Char('q') => return Ok(()),
            _ => {}
        }
    }
}

fn read_key() -> Result<KeyCode> {
    loop {
        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                return Ok(key.code);
            }
        }
    }
}

pub async fn with_terminal<T>(action: impl AsyncFnOnce(&mut Tui) -> Result<T>) -> Result<T> {
    let mut terminal = setup_terminal()?;
    let result = action(&mut terminal).await;
    restore_terminal(&mut terminal)?;
    result
}

fn setup_terminal() -> Result<Tui> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;
    terminal.clear()?;
    terminal.hide_cursor()?;
    Ok(terminal)
}

fn setup_terminal_in_place(terminal: &mut Tui) -> Result<()> {
    enable_raw_mode()?;
    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
    terminal.clear()?;
    terminal.hide_cursor()?;
    Ok(())
}

fn restore_terminal(terminal: &mut Tui) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn draw_login(frame: &mut ratatui::Frame, methods: &[login::LoginMethod], selected: usize, last_used: Option<&login::LoginMethod>) {
    let area = centered_rect(frame.area(), 60, 50);
    let items = methods.iter().map(|method| {
        let label = if last_used == Some(method) { format!("{method} (last used)") } else { method.to_string() };
        ListItem::new(label)
    }).collect::<Vec<_>>();
    let list = List::new(items)
        .block(Block::default().title("Welcome to KSeF Bot. How would you like to log in? ➜🚪").borders(Borders::ALL))
        .highlight_symbol("> ")
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));
    let mut state = ListState::default();
    state.select(Some(selected));
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_main_menu(frame: &mut ratatui::Frame, actions: &[MainMenuAction], selected: usize) {
    let area = centered_rect(frame.area(), 60, 60);
    let items = actions.iter().map(|action| ListItem::new(action.to_string())).collect::<Vec<_>>();
    let list = List::new(items)
        .block(Block::default()
        .title("What shall we do now?:")
        .borders(Borders::ALL))
        .highlight_symbol("> ")
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));
    let mut state = ListState::default();
    state.select(Some(selected));
    frame.render_stateful_widget(list, area, &mut state);
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage((100 - height) / 2), Constraint::Percentage(height), Constraint::Percentage((100 - height) / 2)])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage((100 - width) / 2), Constraint::Percentage(width), Constraint::Percentage((100 - width) / 2)])
        .split(vertical[1])[1]
}

fn draw_pause(frame: &mut ratatui::Frame) {
    let area = centered_rect(frame.area(), 60, 20);
    let paragraph = Paragraph::new("Press [Enter] to go back to the main menu...").block(Block::default().title("Done").borders(Borders::ALL));
    frame.render_widget(paragraph, area);
}
