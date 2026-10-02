use crate::api::users::AppUser;
use crate::login::AuthUser;
use crate::login;
use anyhow::Result;
use chrono::{Datelike, Months, NaiveDate};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    buffer::Buffer,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, ListState},
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
#[path = "workspace.rs"]
mod workspace;
#[path = "history.rs"]
mod history;

pub struct Tui {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
    backdrop: Option<Buffer>,
}

impl Tui {
    fn new() -> Result<Self> {
        Ok(Self { terminal: ratatui::try_init()?, backdrop: None })
    }

    pub fn draw(&mut self, draw: impl FnOnce(&mut ratatui::Frame)) -> io::Result<()> {
        self.terminal.draw(|frame| {
            if let Some(backdrop) = &self.backdrop
                && backdrop.area == frame.area() {
                *frame.buffer_mut() = backdrop.clone();
            }
            draw(frame);
        })?;
        Ok(())
    }

    fn draw_workspace(&mut self, draw: impl FnOnce(&mut ratatui::Frame)) -> io::Result<()> {
        let mut backdrop = None;
        self.terminal.draw(|frame| {
            draw(frame);
            backdrop = Some(frame.buffer_mut().clone());
        })?;
        self.backdrop = backdrop;
        Ok(())
    }
}

fn draw_login(frame: &mut ratatui::Frame, methods: &[login::LoginMethod], selected: usize, last_used: Option<&login::LoginMethod>) {
    let Some(area) = widgets::modal(frame, "Sign in to KSeF Bot", 76, methods.len() as u16,
        "↑↓:move | Enter:select | Ctrl+C:quit") else { return; };
    let items: Vec<_> = methods.iter().map(|method| {
        let prefix = if last_used == Some(method) { "(last used) " } else { "" };
        let width = area.width.saturating_sub(2 + prefix.len() as u16);
        ListItem::new(Line::from(vec![
            Span::styled(prefix, Style::default().fg(Color::LightGreen).add_modifier(Modifier::BOLD)),
            Span::raw(widgets::clipped_line(&method.to_string(), width)),
        ]))
    }).collect();
    frame.render_stateful_widget(List::new(items).highlight_symbol(widgets::SELECTOR), area,
        &mut ListState::default().with_selected(Some(selected)));
}

pub async fn login_loop(terminal: &mut Tui) -> Result<AuthUser> {
    let methods: Vec<_> = login::LoginMethod::iter().collect();
    let last_used = login::last_used_method();
    let mut selected = methods.iter().position(|method| Some(method) == last_used.as_ref()).unwrap_or_default();
    loop {
        terminal.draw(|frame| draw_login(frame, &methods, selected, last_used.as_ref()))?;
        match read_key()? {
            KeyCode::Enter => {}
            key @ (KeyCode::Up | KeyCode::Down) => {
                widgets::navigate(&mut selected, key, methods.len());
                continue;
            }
            _ => continue,
        }
        let method = &methods[selected];
        if let Some(user) = login::try_resume_method(method).await {
            return Ok(user);
        }
        let user = match method {
            login::LoginMethod::Google => login::login_with_google().await,
            login::LoginMethod::Microsoft => login::login_with_microsoft().await,
            login::LoginMethod::Phone => {
                ratatui::try_restore()?;
                let result = login::login_with_phone_loop().await;
                *terminal = Tui::new()?;
                result
            }
        };
        match user {
            Ok(user) => return Ok(user),
            Err(error) => widgets::message(terminal, "Sign-in failed", &[format!("{error:#}")])?,
        }
    }
}

pub(crate) async fn main_loop(terminal: &mut Tui, app_user: &AppUser) -> Result<()> {
    workspace::run(terminal, app_user).await
}

fn month_range(today: NaiveDate, months_ago: u32) -> (NaiveDate, NaiveDate) {
    let start = today.with_day(1).unwrap().checked_sub_months(Months::new(months_ago)).unwrap();
    let end = start.checked_add_months(Months::new(1)).unwrap().pred_opt().unwrap();
    (start, end)
}

fn read_key() -> Result<KeyCode> {
    loop {
        match event::read()? {
            Event::Resize(_, _) => return Ok(KeyCode::Null),
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c' | 'C')) {
                    ratatui::try_restore()?;
                    std::process::exit(130);
                }
                let code = key.code;
                let (width, height) = crossterm::terminal::size()?;
                return Ok(if (width < 48 || height < 24) && code != KeyCode::Esc { KeyCode::Null } else { code });
            }
            _ => {}
        }
    }
}

pub async fn with_terminal<T>(action: impl AsyncFnOnce(&mut Tui) -> Result<T>) -> Result<T> {
    let mut terminal = Tui::new()?;
    let result = action(&mut terminal).await;
    ratatui::try_restore()?;
    result
}
