use crate::{MainMenuAction, login, customers, invoices, settings};
use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState},
};
use std::io;
use ratatui::widgets::Paragraph;
use strum::IntoEnumIterator;
use crate::login::AuthUser;

type Tui = Terminal<CrosstermBackend<io::Stdout>>;

pub async fn run() -> Result<()> {
    let mut terminal = setup_terminal()?;
    let result = async {
        let logged_user = login_loop(&mut terminal).await?;
        main_loop(&mut terminal, &logged_user).await?;
        Ok(())
    }.await;
    restore_terminal(&mut terminal)?;
    result
}

async fn login_loop(terminal: &mut Tui) -> Result<AuthUser> {
    let methods: Vec<login::LoginMethod> = login::LoginMethod::iter().collect();
    let mut selected = 0usize;
    loop {
        terminal.draw(|frame| draw_login(frame, &methods, selected))?;
        match read_key()? {
            KeyCode::Up | KeyCode::Char('k') => selected = selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                if selected + 1 < methods.len() {
                    selected += 1;
                }
            }
            KeyCode::Enter => {
                restore_terminal(terminal)?;
                let user = match methods[selected].clone() {
                    login::LoginMethod::Google => login::login_with_google().await?,
                    login::LoginMethod::Microsoft => login::login_with_microsoft().await?,
                    login::LoginMethod::Email => login::login_with_email_loop().await?
                };
                setup_terminal_in_place(terminal)?;
                return Ok(user);
            }
            KeyCode::Esc | KeyCode::Char('q') => return Err(anyhow::anyhow!("Login cancelled")), _ => {}
        }
    }
}

async fn main_loop(terminal: &mut Tui, logged_user: &AuthUser) -> Result<()> {
    println!("Logged user: {}", logged_user.email);
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
                restore_terminal(terminal)?;
                match actions[selected].clone() {
                    MainMenuAction::CreateSalesInvoice => invoices::create_sales_invoice().await?,
                    MainMenuAction::ListSalesInvoices => invoices::list_sales_invoices().await?,
                    MainMenuAction::ListPurchaseInvoices => invoices::list_purchase_invoices().await?,
                    MainMenuAction::CreateCustomer => customers::create_customer().await?,
                    MainMenuAction::ListCustomers => customers::list_customers().await?,
                    MainMenuAction::UserSettings => settings::edit_profile().await?,
                    MainMenuAction::Exit => return Ok(())
                }
                setup_terminal_in_place(terminal)?;
                pause(terminal)?;
            }
            KeyCode::Esc | KeyCode::Char('q') => return Ok(()),
            _ => {}
        }
    }
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

fn draw_login(frame: &mut ratatui::Frame, methods: &[login::LoginMethod], selected: usize) {
    let area = centered_rect(frame.area(), 60, 50);
    let items = methods.iter().map(|method| ListItem::new(method.to_string())).collect::<Vec<_>>();
    let list = List::new(items)
        .block(Block::default().title("Welcome to KSeF Bot. How would you like to log in?").borders(Borders::ALL), )
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
        .block(Block::default().title("What shall we do now?").borders(Borders::ALL))
        .highlight_symbol("> ")
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));
    let mut state = ListState::default();
    state.select(Some(selected));
    frame.render_stateful_widget(list, area, &mut state);
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let vertical = Layout::default().direction(Direction::Vertical).constraints([
        Constraint::Percentage((100 - height) / 2),
        Constraint::Percentage(height),
        Constraint::Percentage((100 - height) / 2),
    ]).split(area);
    Layout::default().direction(Direction::Horizontal).constraints([
        Constraint::Percentage((100 - width) / 2),
        Constraint::Percentage(width),
        Constraint::Percentage((100 - width) / 2),
    ]).split(vertical[1])[1]
}

fn draw_pause(frame: &mut ratatui::Frame) {
    let area = centered_rect(frame.area(), 60, 20);
    let paragraph = Paragraph::new("Press [Enter] to go back to the main menu...")
        .block(Block::default().title("Done").borders(Borders::ALL), );
    frame.render_widget(paragraph, area);
}