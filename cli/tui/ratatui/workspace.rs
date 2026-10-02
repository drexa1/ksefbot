use super::{Tui, customers, history::{self, InvoiceHistory}, invoices::{self, InvoiceType}, month_range, read_key, users, widgets};
use crate::api::{customers::AppContractor, users::AppUser};
use anyhow::Result;
use chrono::{Local, NaiveDate};
use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState, Padding, Paragraph},
};
use serde_json::Value;
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Section { Invoices, Customers, Settings }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Focus { Cards, Customers, Settings }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action { NewInvoice, Download, NewCustomer, EditCustomer, Back, Save, Discard }

enum LoadState { Pending, Ready, Failed(String) }

struct Workspace {
    section: Section,
    focus: Focus,
    user: AppUser,
    draft: AppUser,
    customers: Vec<AppContractor>,
    customer_state: LoadState,
    customer_selection: ListState,
    customer_details: bool,
    customer_scroll: u16,
    customer_height: u16,
    history: InvoiceHistory,
    history_state: LoadState,
    older: bool,
    history_exhausted: bool,
    invoice_type: InvoiceType,
    selected_month: usize,
    selected_invoice: usize,
    card_start: usize,
    card_capacity: usize,
    show_latest: bool,
    settings_selection: ListState,
}

impl Workspace {
    fn new(user: &AppUser) -> Self {
        let (start, end) = history::initial_range(Local::now().date_naive());
        Self {
            section: Section::Invoices, focus: Focus::Cards,
            user: user.clone(), draft: user.clone(),
            customers: Vec::new(), customer_state: LoadState::Pending, customer_selection: ListState::default(),
            customer_details: false, customer_scroll: 0, customer_height: 1,
            history: InvoiceHistory { months: Vec::new(), start, end }, history_state: LoadState::Pending, older: false,
            history_exhausted: false, invoice_type: InvoiceType::Sales, selected_month: 0, selected_invoice: 0,
            card_start: 0, card_capacity: 1, show_latest: true,
            settings_selection: ListState::default().with_selected(Some(0)),
        }
    }

    fn month_indices(&self) -> Vec<usize> {
        self.history.months.iter().enumerate().filter(|(_, month)| !month.invoices(self.invoice_type).is_empty())
            .map(|(index, _)| index).collect()
    }

    fn selected_date(&self) -> Option<NaiveDate> {
        self.month_indices().get(self.selected_month).map(|index| self.history.months[*index].month)
    }

    fn invoice(&self) -> Option<&Value> {
        self.month_indices().get(self.selected_month)
            .and_then(|index| self.history.months[*index].invoices(self.invoice_type).get(self.selected_invoice))
    }

    fn customer(&self) -> Option<&AppContractor> {
        self.customer_selection.selected().and_then(|index| self.customers.get(index))
    }

    fn set_focus(&mut self, focus: Focus) {
        self.focus = focus;
        self.section = match focus {
            Focus::Customers => Section::Customers,
            Focus::Settings => Section::Settings,
            Focus::Cards => Section::Invoices,
        };
    }

    fn cycle_focus(&mut self, backwards: bool) {
        self.show_latest = false;
        let last = self.month_indices().len().saturating_sub(1);
        match (self.focus, backwards) {
            (Focus::Cards, false) if self.selected_month < last => self.selected_month += 1,
            (Focus::Cards, true) if self.selected_month > 0 => self.selected_month -= 1,
            (Focus::Cards, false) => self.set_focus(Focus::Customers),
            (Focus::Cards, true) => self.set_focus(Focus::Settings),
            (Focus::Customers, false) => self.set_focus(Focus::Settings),
            (Focus::Customers, true) => {
                self.selected_month = last;
                self.set_focus(Focus::Cards);
            }
            (Focus::Settings, false) => {
                self.selected_month = 0;
                self.set_focus(Focus::Cards);
            }
            (Focus::Settings, true) => self.set_focus(Focus::Customers),
        }
        self.selected_invoice = 0;
    }

    fn set_type(&mut self, kind: InvoiceType) {
        self.set_focus(Focus::Cards);
        if self.invoice_type == kind { return; }
        self.invoice_type = kind;
        if matches!(self.history_state, LoadState::Failed(_)) { self.history_state = LoadState::Pending; }
        self.history_exhausted = false;
        self.restore_month(None);
    }

    fn restore_month(&mut self, date: Option<NaiveDate>) {
        let indices = self.month_indices();
        let selected = indices.iter().position(|index| Some(self.history.months[*index].month) == date);
        self.show_latest = selected.is_none();
        self.selected_month = selected.unwrap_or_else(|| indices.len().saturating_sub(self.card_capacity));
        self.selected_invoice = 0;
        self.card_start = self.selected_month.saturating_sub(self.card_capacity.saturating_sub(1));
    }

    fn card_range(&mut self, width: u16) -> Range<usize> {
        self.card_capacity = usize::from(((width + 1) / 39).max(1));
        let count = self.month_indices().len();
        if self.show_latest && count > 0 {
            self.selected_month = count.saturating_sub(self.card_capacity);
            self.card_start = self.selected_month;
            self.show_latest = false;
        }
        self.selected_month = self.selected_month.min(count.saturating_sub(1));
        self.card_start = self.card_start.min(count.saturating_sub(self.card_capacity));
        if self.selected_month < self.card_start { self.card_start = self.selected_month; }
        if self.selected_month >= self.card_start + self.card_capacity {
            self.card_start = self.selected_month + 1 - self.card_capacity;
        }
        self.card_start..(self.card_start + self.card_capacity).min(count)
    }

    fn move_card(&mut self, key: KeyCode) {
        self.show_latest = false;
        let count = self.month_indices().len();
        if matches!(key, KeyCode::Left | KeyCode::PageUp) && self.selected_month == 0
            && !self.history_exhausted && matches!(self.history_state, LoadState::Ready) {
            if history::older_range(self.history.start).is_none() {
                self.history_exhausted = true;
                return;
            }
            self.older = true;
            self.history_state = LoadState::Pending;
            return;
        }
        let previous = self.selected_month;
        match key {
            KeyCode::Left => self.selected_month = self.selected_month.saturating_sub(1),
            KeyCode::Right => self.selected_month = (self.selected_month + 1).min(count.saturating_sub(1)),
            KeyCode::Home => self.selected_month = 0,
            KeyCode::End => self.selected_month = count.saturating_sub(1),
            KeyCode::PageUp => self.selected_month = self.selected_month.saturating_sub(self.card_capacity),
            KeyCode::PageDown => self.selected_month = (self.selected_month + self.card_capacity).min(count.saturating_sub(1)),
            _ => {}
        }
        if previous != self.selected_month { self.selected_invoice = 0; }
        if let Some(index) = self.month_indices().get(self.selected_month) {
            let count = self.history.months[*index].invoices(self.invoice_type).len();
            match key {
                KeyCode::Up => self.selected_invoice = self.selected_invoice.saturating_sub(1),
                KeyCode::Down => self.selected_invoice = (self.selected_invoice + 1).min(count.saturating_sub(1)),
                _ => {}
            }
        }
    }

    fn move_customer(&mut self, key: KeyCode) {
        if self.customer_details {
            scroll(&mut self.customer_scroll, key, self.customer_height);
        } else {
            let mut index = self.customer_selection.selected().unwrap_or_default();
            widgets::navigate(&mut index, key, self.customers.len());
            self.customer_selection.select((!self.customers.is_empty()).then_some(index));
        }
    }

    fn open_customer(&mut self) {
        if self.customer().is_some() {
            self.customer_details = true;
            self.customer_scroll = 0;
        }
    }

    async fn load(&mut self, terminal: &mut Tui) -> Result<()> {
        if matches!(self.customer_state, LoadState::Pending) {
            terminal.draw_workspace(|frame| self.draw(frame))?;
            let id = self.customer().map(|customer| customer.id.clone());
            match customers::load_customers(&self.user).await {
                Ok(rows) => {
                    let index = rows.iter().position(|customer| Some(&customer.id) == id.as_ref()).unwrap_or_default();
                    self.customer_selection.select((!rows.is_empty()).then_some(index));
                    self.customers = rows;
                    self.customer_state = LoadState::Ready;
                }
                Err(error) => self.customer_state = LoadState::Failed(format!("{error:#}")),
            }
        }
        if matches!(self.history_state, LoadState::Pending) {
            terminal.draw_workspace(|frame| self.draw(frame))?;
            let result = self.load_history().await;
            self.history_state = match result {
                Ok(()) => LoadState::Ready,
                Err(error) => LoadState::Failed(error.root_cause().to_string()),
            };
        }
        Ok(())
    }

    async fn load_history(&mut self) -> Result<()> {
        let date = self.selected_date();
        if self.older {
            let (start, end) = history::older_range(self.history.start).ok_or_else(|| anyhow::anyhow!("No earlier supported months"))?;
            let earlier = history::load(&self.user, start, end).await?;
            self.extend_history(earlier);
        } else {
            self.history = history::load(&self.user, self.history.start, Local::now().date_naive()).await?;
            self.history_exhausted = false;
            self.restore_month(date);
        }
        self.older = false;
        Ok(())
    }

    fn extend_history(&mut self, mut earlier: InvoiceHistory) {
        let previous = self.selected_date();
        let selected_invoice = self.selected_invoice;
        let date = earlier.months.iter().rev().find(|month| !month.invoices(self.invoice_type).is_empty()).map(|month| month.month);
        self.history_exhausted = earlier.months.is_empty();
        earlier.months.append(&mut self.history.months);
        earlier.end = self.history.end;
        self.history = earlier;
        self.restore_month(date.or(previous));
        if date.is_none() { self.selected_invoice = selected_invoice; }
    }

    fn draw(&mut self, frame: &mut Frame) {
        let screen = frame.area();
        frame.render_widget(Block::default(), screen);
        if screen.width < widgets::MIN_WIDTH || screen.height < widgets::MIN_HEIGHT {
            widgets::modal(frame, "KSeF Bot", 48, 1, "");
            return;
        }
        let area = Rect::new(screen.x + 2, screen.y, screen.width - 4, screen.height);
        let sections = Layout::vertical([
            Constraint::Length(3), Constraint::Length(1), Constraint::Length(((screen.height - 5) / 2).clamp(8, 12)),
            Constraint::Length(1), Constraint::Min(7), Constraint::Length(1),
        ]).split(area);
        self.draw_type(frame, sections[0]);
        self.draw_cards(frame, sections[2]);
        let lower = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).spacing(1).split(sections[4]);
        self.draw_customers(frame, lower[0]);
        self.draw_settings(frame, lower[1]);
        frame.render_widget(Paragraph::new(self.footer(sections[5].width)), sections[5]);
    }

    fn footer(&self, width: u16) -> String {
        let hint = match self.focus {
            Focus::Cards => "←/→:months | ↑/↓:invoice | Enter:details | d:download",
            Focus::Customers if self.customer_details => "↑/↓:scroll | n:new | e:edit | Esc/q:list",
            Focus::Customers => "↑/↓:customer | Enter:details | n:new",
            Focus::Settings if self.user != self.draft => "↑/↓:field | Enter:edit | w:save | x:discard",
            Focus::Settings => "↑/↓:field | Enter:edit",
        };
        let create = if self.section == Section::Invoices && self.invoice_type == InvoiceType::Sales && width >= 85 { " | n:new" } else { "" };
        let tab = if width >= 120 { "Tab/Shift+Tab:next/previous" } else { "Tab:next" };
        widgets::clipped_line(&format!("s/p:invoice type | {tab} | Ctrl+C:quit{create} | {hint}"), width)
    }

    fn draw_type(&self, frame: &mut Frame, area: Rect) {
        let mut x = area.x;
        for (kind, label, emoji) in [(InvoiceType::Sales, "[(S)ales]", "💵"), (InvoiceType::Purchases, "[(P)urchases]", "🛒")] {
            let selected = self.invoice_type == kind;
            let text = if selected { format!("{label} {emoji} ") } else { label.to_string() };
            let width = Line::from(text.as_str()).width() as u16;
            let style = if selected { bold().bg(self.invoice_color()).fg(Color::Black) } else { Style::default() };
            let tab = Rect::new(x, area.y + 1, width.min(area.right().saturating_sub(x)), 1);
            frame.render_widget(Paragraph::new(text).style(style), tab);
            x += width + 2;
        }
        let status = match &self.history_state {
            LoadState::Pending => "Loading...".to_string(),
            LoadState::Failed(error) => format!("Load failed: {error}"),
            LoadState::Ready => String::new(),
        };
        let status_area = Rect::new(x, area.y + 1, area.right().saturating_sub(x), 1);
        frame.render_widget(Paragraph::new(widgets::clipped_line(&status, status_area.width)).alignment(Alignment::Right), status_area);
    }

    fn draw_cards(&mut self, frame: &mut Frame, area: Rect) {
        let indices = self.month_indices();
        let range = self.card_range(area.width);
        if range.is_empty() {
            let title = format!("{} invoices", self.invoice_type);
            let focused = self.focus == Focus::Cards;
            let inner = card_pane(frame, area, &title, focused, self.card_color(focused));
            let status = match &self.history_state {
                LoadState::Pending => "Loading invoice history...".to_string(),
                LoadState::Failed(_) => String::new(),
                LoadState::Ready => format!("No {} invoices from {} to {}.{}", self.invoice_type, self.history.start, self.history.end,
                    if self.history_exhausted { "" } else { "\nPress Left to search earlier months." }),
            };
            draw_text(frame, inner, &[status], &mut 0);
            return;
        }
        let cards = Layout::horizontal(vec![Constraint::Ratio(1, range.len() as u32); range.len()]).spacing(1).split(area);
        for (slot, position) in range.enumerate() {
            let month = &self.history.months[indices[position]];
            let records = month.invoices(self.invoice_type);
            let selected = position == self.selected_month;
            let record_index = if selected { self.selected_invoice.min(records.len() - 1) } else { 0 };
            let invoice = &records[record_index];
            let card = cards[slot];
            let title = format!("📅 {}: {} invoice(s)", month.month.format("%B %Y"), records.len());
            let focused = selected && self.focus == Focus::Cards;
            let inner = card_pane(frame, card, &title, focused, self.card_color(focused));
            let body = &invoice["InvoiceBody"];
            let number = body["InvoiceNumber"].as_str().unwrap_or("-");
            let party_key = if self.invoice_type == InvoiceType::Sales { "Buyer" } else { "Seller" };
            let party = invoice[party_key]["IdentificationData"]["Name"].as_str().unwrap_or("-");
            let currency = body["CurrencyCode"].as_str().unwrap_or("-");
            let gross = body["TotalGrossAmount"].as_f64().unwrap_or_default();
            let lines = vec![
                number.to_string(), party.to_string(),
                format!("Issued: {}", body["IssueDate"].as_str().unwrap_or("-")), format!("{gross:.2} {currency}"),
            ];
            let body_area = Rect::new(inner.x, inner.y, inner.width, inner.height.saturating_sub(1));
            frame.render_widget(Paragraph::new(lines.into_iter().map(|line| Line::from(widgets::clipped_line(&line, inner.width))).collect::<Vec<_>>()), body_area);
            frame.render_widget(Paragraph::new("[(D)ownload]")
                .style(if focused { bold() } else { Style::default() }),
                Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1));
        }
    }

    fn invoice_color(&self) -> Color {
        match self.invoice_type {
            InvoiceType::Sales => Color::Rgb(100, 149, 237),
            InvoiceType::Purchases => Color::Rgb(255, 165, 0),
        }
    }

    fn card_color(&self, focused: bool) -> Color {
        if focused { return self.invoice_color(); }
        match self.invoice_type {
            InvoiceType::Sales => Color::Rgb(190, 212, 250),
            InvoiceType::Purchases => Color::Rgb(255, 220, 170),
        }
    }

    fn draw_customers(&mut self, frame: &mut Frame, area: Rect) {
        let title = if self.customer_details { "💼 Customer details" } else { "💼 Customers" };
        let inner = pane(frame, area, title, self.section == Section::Customers);
        let parts = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(inner);
        if let LoadState::Failed(error) = &self.customer_state {
            self.customer_height = draw_text(frame, parts[0], &[format!("Load failed: {error}\nr: retry")], &mut self.customer_scroll);
        } else if matches!(self.customer_state, LoadState::Pending) {
            frame.render_widget(Paragraph::new("Loading customers..."), parts[0]);
        } else if self.customer_details {
            let lines = self.customer().map(customers::preview).unwrap_or_else(|| vec!["No customer selected.".to_string()]);
            self.customer_height = draw_fields(frame, parts[0], &lines, &mut self.customer_scroll);
        } else if self.customers.is_empty() {
            frame.render_widget(Paragraph::new("No customers found."), parts[0]);
        } else {
            let labels: Vec<_> = self.customers.iter().map(|customer| customer.name.clone()).collect();
            draw_list(frame, parts[0], &labels, &mut self.customer_selection);
        }
        let mut buttons = vec!["(N)ew"];
        if self.customer_details {
            if self.customer().is_some() { buttons.push("(E)dit"); }
            buttons.push("(Esc) Back");
        }
        draw_buttons(frame, parts[1], &buttons);
    }

    fn draw_settings(&mut self, frame: &mut Frame, area: Rect) {
        let title = format!("🧑‍💻 User details{}",
            if self.user == self.draft { "" } else { " * unsaved" });
        let inner = pane(frame, area, &title, self.section == Section::Settings);
        let parts = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(inner);
        draw_list(frame, parts[0], &users::fields(&self.draft), &mut self.settings_selection);
        if self.user != self.draft {
            draw_buttons(frame, parts[1], &["(W) Save", "(X) Discard"]);
        } else {
            frame.render_widget(Paragraph::new("Enter: edit field"), parts[1]);
        }
    }
}

fn bold() -> Style { Style::default().add_modifier(Modifier::BOLD) }

fn card_pane(frame: &mut Frame, area: Rect, title: &str, focused: bool, color: Color) -> Rect {
    frame.render_widget(Block::default().style(Style::default().bg(color).fg(Color::White)), area);
    let width = area.width.saturating_sub(2);
    frame.render_widget(Paragraph::new(widgets::clipped_line(title, width)).style(if focused { bold() } else { Style::default() }),
        Rect::new(area.x + 1, area.y + 1, width, 1));
    Rect::new(area.x + 1, area.y + 2, width, area.height.saturating_sub(3))
}

fn pane(frame: &mut Frame, area: Rect, title: &str, focused: bool) -> Rect {
    let block = widgets::block().title(Line::from(format!(" {title} ")).style(if focused { bold() } else { Style::default() }))
        .padding(Padding::new(1, 1, 1, 0))
        .border_style(if focused { bold() } else { Style::default() });
    let inner = block.inner(area);
    frame.render_widget(block, area);
    inner
}

fn draw_list(frame: &mut Frame, area: Rect, labels: &[String], state: &mut ListState) {
    let items: Vec<_> = labels.iter().map(|label| ListItem::new(widgets::clipped_line(label, area.width.saturating_sub(2)))).collect();
    frame.render_stateful_widget(List::new(items).highlight_symbol(widgets::SELECTOR), area, state);
}

fn draw_buttons(frame: &mut Frame, area: Rect, buttons: &[&str]) {
    let columns = Layout::horizontal(vec![Constraint::Ratio(1, buttons.len() as u32); buttons.len()]).split(area);
    for (label, column) in buttons.iter().zip(columns.iter()) {
        frame.render_widget(Paragraph::new(format!("[{label}]")).alignment(Alignment::Center), *column);
    }
}

fn draw_text(frame: &mut Frame, area: Rect, lines: &[String], offset: &mut u16) -> u16 {
    draw_lines(frame, area, widgets::wrap_lines(&lines.join("\n"), area.width).into_iter().map(Line::from).collect(), offset)
}

fn draw_fields(frame: &mut Frame, area: Rect, lines: &[String], offset: &mut u16) -> u16 {
    let lines = lines.iter().flat_map(|source| {
        let key = source.split_once(':').map(|(key, _)| format!("{key}:"));
        widgets::wrap_lines(source, area.width).into_iter().enumerate().map(move |(index, line)| {
            let key = key.as_deref().filter(|key| index == 0 && line.starts_with(key)).unwrap_or("");
            Line::from(vec![Span::styled(key.to_string(), bold()), Span::raw(line[key.len()..].to_string())])
        })
    }).collect();
    draw_lines(frame, area, lines, offset)
}

fn draw_lines(frame: &mut Frame, area: Rect, lines: Vec<Line<'_>>, offset: &mut u16) -> u16 {
    *offset = (*offset).min(lines.len().saturating_sub(area.height as usize).min(u16::MAX as usize) as u16);
    frame.render_widget(Paragraph::new(lines).scroll((*offset, 0)), area);
    area.height.max(1)
}

fn scroll(offset: &mut u16, key: KeyCode, page: u16) {
    match key {
        KeyCode::Down => *offset = offset.saturating_add(1),
        KeyCode::Up => *offset = offset.saturating_sub(1),
        KeyCode::PageDown => *offset = offset.saturating_add(page),
        KeyCode::PageUp => *offset = offset.saturating_sub(page),
        KeyCode::Home => *offset = 0,
        KeyCode::End => *offset = u16::MAX,
        _ => {}
    }
}

async fn activate(app: &mut Workspace, terminal: &mut Tui, action: Action) -> Result<()> {
    let result = match action {
        Action::Back => { app.customer_details = false; app.set_focus(Focus::Customers); return Ok(()); }
        Action::NewCustomer => {
            let result = customers::create_customer(terminal, &app.user).await;
            app.customer_state = LoadState::Pending;
            result
        }
        Action::EditCustomer => {
            let Some(customer) = app.customer().cloned() else { return Ok(()); };
            let result = customers::edit_customer(terminal, &app.user, &customer).await;
            if !result.as_ref().is_ok_and(Vec::is_empty) { app.customer_state = LoadState::Pending; }
            result
        }
        Action::NewInvoice => {
            let result = invoices::prompt_create_invoice(terminal, &app.user).await;
            app.older = false;
            app.history_state = LoadState::Pending;
            result
        }
        Action::Download => {
            let Some(invoice) = app.invoice() else { return Ok(()); };
            let number = invoice["InvoiceBody"]["InvoiceNumber"].as_str().unwrap_or("-");
            let (from, to) = month_range(app.selected_date().unwrap(), 0);
            invoices::download_invoice_xml(&app.user, &app.invoice_type, invoice, number, &from.format("%Y/%m/%d").to_string(), &to.format("%Y/%m/%d").to_string()).await
                .map(|path| vec![format!("Invoice XML saved to {}", path.display())])
        }
        Action::Save => users::save_profile(terminal, &mut app.user, &app.draft).await,
        Action::Discard => {
            if widgets::confirm(terminal, "Discard unsaved settings?", false)? { app.draft = app.user.clone(); }
            return Ok(());
        }
    };
    match result {
        Ok(lines) => widgets::message(terminal, "Result", &lines)?,
        Err(error) => widgets::message(terminal, "Action failed", &[format!("{error:#}")])?,
    }
    Ok(())
}

pub async fn run(terminal: &mut Tui, user: &AppUser) -> Result<()> {
    let mut app = Workspace::new(user);
    loop {
        app.load(terminal).await?;
        terminal.draw_workspace(|frame| app.draw(frame))?;
        let key = read_key()?;
        match key {
            KeyCode::Char('s' | 'S') => app.set_type(InvoiceType::Sales),
            KeyCode::Char('p' | 'P') => app.set_type(InvoiceType::Purchases),
            KeyCode::Tab => app.cycle_focus(false),
            KeyCode::BackTab => app.cycle_focus(true),
            KeyCode::Char('n' | 'N') if app.section == Section::Invoices && app.invoice_type == InvoiceType::Sales => activate(&mut app, terminal, Action::NewInvoice).await?,
            KeyCode::Char('n' | 'N') if app.focus == Focus::Customers => activate(&mut app, terminal, Action::NewCustomer).await?,
            KeyCode::Char('e' | 'E') if app.focus == Focus::Customers && app.customer_details => activate(&mut app, terminal, Action::EditCustomer).await?,
            KeyCode::Char('w' | 'W') if app.focus == Focus::Settings && app.user != app.draft => activate(&mut app, terminal, Action::Save).await?,
            KeyCode::Char('x' | 'X') if app.focus == Focus::Settings && app.user != app.draft => activate(&mut app, terminal, Action::Discard).await?,
            KeyCode::Char('d' | 'D') if app.section == Section::Invoices => activate(&mut app, terminal, Action::Download).await?,
            KeyCode::Char('r') => {
                if matches!(app.customer_state, LoadState::Failed(_)) { app.customer_state = LoadState::Pending; }
            }
            KeyCode::Esc | KeyCode::Char('q') if app.section == Section::Customers && app.customer_details => {
                activate(&mut app, terminal, Action::Back).await?;
            }
            KeyCode::Esc | KeyCode::Char('q') => {}
            KeyCode::Enter => match app.focus {
                Focus::Cards => if let Some(invoice) = app.invoice() {
                    widgets::message(terminal, "Invoice details", &invoices::invoice_preview(invoice, &app.invoice_type))?;
                },
                Focus::Customers => app.open_customer(),
                Focus::Settings => {
                    if let Some(index) = app.settings_selection.selected() { users::edit_field(terminal, &mut app.draft, index)?; }
                }
            },
            _ if app.focus == Focus::Cards => app.move_card(key),
            _ if app.focus == Focus::Customers => app.move_customer(key),
            _ if app.focus == Focus::Settings => {
                let mut index = app.settings_selection.selected().unwrap_or_default();
                widgets::navigate(&mut index, key, users::fields(&app.draft).len());
                app.settings_selection.select(Some(index));
            }
            _ => {}
        }
    }
}
