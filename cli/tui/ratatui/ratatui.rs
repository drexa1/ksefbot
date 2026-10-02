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
    let mut terminal = Tui::new()?;
    let result = action(&mut terminal).await;
    ratatui::try_restore()?;
    result
}
