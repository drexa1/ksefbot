use super::{Tui, customers, history::{self, InvoiceHistory}, invoices::{self, InvoiceType}, month_range, read_key, users, widgets};
use crate::api::{customers::AppContractor, users::AppUser};
use anyhow::Result;
use chrono::{Datelike, Local, Months, NaiveDate};
use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, BorderType, List, ListItem, ListState, Padding, Paragraph},
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
    invoice_type: InvoiceType,
    selected_month: usize,
    selected_invoice: usize,
    card_start: usize,
    card_capacity: usize,
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
            invoice_type: InvoiceType::Sales, selected_month: 0, selected_invoice: 0,
            card_start: 0, card_capacity: 1,
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
        let date = self.selected_date();
        self.invoice_type = kind;
        self.restore_month(date);
    }

    fn restore_month(&mut self, date: Option<NaiveDate>) {
        let indices = self.month_indices();
        self.selected_month = indices.iter().position(|index| Some(self.history.months[*index].month) == date)
            .unwrap_or_default();
        self.selected_invoice = 0;
        self.card_start = self.selected_month.saturating_sub(self.card_capacity.saturating_sub(1));
    }

    fn card_range(&mut self, width: u16) -> Range<usize> {
        self.card_capacity = usize::from(((width + 1) / 31).max(1));
        let count = self.month_indices().len();
        self.selected_month = self.selected_month.min(count.saturating_sub(1));
        self.card_start = self.card_start.min(count.saturating_sub(self.card_capacity));
        if self.selected_month < self.card_start { self.card_start = self.selected_month; }
        if self.selected_month >= self.card_start + self.card_capacity {
            self.card_start = self.selected_month + 1 - self.card_capacity;
        }
        self.card_start..(self.card_start + self.card_capacity).min(count)
    }

    fn move_card(&mut self, key: KeyCode) {
        let count = self.month_indices().len();
        if matches!(key, KeyCode::Left | KeyCode::PageUp) && self.selected_month == 0
            && matches!(self.history_state, LoadState::Ready) {
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
            let was_older = self.older;
            let previous_count = self.month_indices().len();
            let previous_start = self.history.start;
            let result = self.load_history().await;
            self.history_state = match result {
                Ok(()) => LoadState::Ready,
                Err(error) => LoadState::Failed(format!("{error:#}")),
            };
            if let LoadState::Failed(error) = &self.history_state {
                widgets::message(terminal, "Invoice history could not be loaded", &[error.clone(), "Press r to retry. Previously loaded invoices are retained.".to_string()])?;
            } else if was_older && previous_count == self.month_indices().len() {
                widgets::message(terminal, "No earlier invoices found", &[format!(
                    "No {} invoices from {} to {}. Scroll left past the first card again to continue searching.",
                    self.invoice_type, self.history.start, previous_start.pred_opt().unwrap(),
                )])?;
            }
        }
        Ok(())
    }

    async fn load_history(&mut self) -> Result<()> {
        let mut date = self.selected_date();
        if self.older {
            let start = self.history.start.checked_sub_months(Months::new(12)).filter(|date| date.year() >= 1)
                .ok_or_else(|| anyhow::anyhow!("No earlier supported months"))?;
            let end = self.history.start.pred_opt().ok_or_else(|| anyhow::anyhow!("No earlier supported months"))?;
            let mut earlier = history::load(&self.user, start, end).await?;
            if let Some(month) = earlier.months.iter().rev().find(|month| !month.invoices(self.invoice_type).is_empty()) {
                date = Some(month.month);
            }
            earlier.months.append(&mut self.history.months);
            earlier.end = self.history.end;
            self.history = earlier;
        } else {
            self.history = history::load(&self.user, self.history.start, Local::now().date_naive()).await?;
        }
        self.older = false;
        self.restore_month(date);
        Ok(())
    }

    fn draw(&mut self, frame: &mut Frame) {
        let screen = frame.area();
        frame.render_widget(Block::default(), screen);
        if screen.width < widgets::MIN_WIDTH || screen.height < widgets::MIN_HEIGHT {
            widgets::modal(frame, "KSeF Bot", 48, 1, "");
            return;
        }
        let area = Rect::new(screen.x + 1, screen.y, screen.width - 2, screen.height);
        let sections = Layout::vertical([
            Constraint::Length(3), Constraint::Length(1), Constraint::Length(((screen.height - 5) / 2).clamp(8, 12)),
            Constraint::Min(7), Constraint::Length(1),
        ]).split(area);
        self.draw_type(frame, sections[0]);
        self.draw_cards(frame, sections[2]);
        let lower = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).spacing(1).split(sections[3]);
        self.draw_customers(frame, lower[0]);
        self.draw_settings(frame, lower[1]);
        frame.render_widget(Paragraph::new(self.footer(sections[4].width)), sections[4]);
    }

    fn footer(&self, width: u16) -> String {
        let hint = match self.focus {
            Focus::Cards => "←/→:months ↑/↓:invoice Enter:details d:download",
            Focus::Customers if self.customer_details => "↑/↓:scroll n:new e:edit Esc:list",
            Focus::Customers => "↑/↓:customer Enter:details n:new",
            Focus::Settings if self.user != self.draft => "↑/↓:field Enter:edit w:save x:discard",
            Focus::Settings => "↑/↓:field Enter:edit",
        };
        let create = if self.section == Section::Invoices && self.invoice_type == InvoiceType::Sales && width >= 85 { " n:new" } else { "" };
        let tab = if width >= 120 { "Tab/Shift+Tab:next/previous" } else { "Tab:next" };
        widgets::clipped_line(&format!("s/c:invoice type {tab} q:quit{create} | {hint}"), width)
    }

    fn draw_type(&self, frame: &mut Frame, area: Rect) {
        let mut x = area.x;
        for (kind, label, emoji) in [(InvoiceType::Sales, "(s) Sales", "💵"), (InvoiceType::Purchases, "(c) Purchases", "🛒")] {
            let selected = self.invoice_type == kind;
            let text = format!(" {label} {} ", if selected { emoji } else { "  " });
            let width = Line::from(text.as_str()).width() as u16;
            let style = if selected { bold().add_modifier(Modifier::REVERSED) } else { Style::default() };
            let tab = Rect::new(x, area.y, width.min(area.right().saturating_sub(x)), area.height);
            frame.render_widget(Block::default().style(style), tab);
            frame.render_widget(Paragraph::new(text).style(style), Rect::new(tab.x, tab.y + 1, tab.width, 1));
            x += width;
        }
        let status = match &self.history_state {
            LoadState::Pending => " Loading...",
            LoadState::Failed(_) => " Failed (r:retry)",
            LoadState::Ready => "",
        };
        frame.render_widget(Paragraph::new(status), Rect::new(x, area.y + 1, area.right().saturating_sub(x), 1));
    }

    fn draw_cards(&mut self, frame: &mut Frame, area: Rect) {
        let indices = self.month_indices();
        let range = self.card_range(area.width);
        if range.is_empty() {
            let title = format!(" {} invoices ", self.invoice_type);
            let inner = pane(frame, area, &title, self.focus == Focus::Cards, Some(self.invoice_color()));
            let status = match &self.history_state {
                LoadState::Pending => "Loading invoice history...".to_string(),
                LoadState::Failed(error) => format!("Load failed: {error}\nPress r to retry."),
                LoadState::Ready => format!("No {} invoices from {} to {}.\nPress Left to search earlier months.", self.invoice_type, self.history.start, self.history.end),
            };
            draw_text(frame, inner, &[status], &mut 0);
            return;
        }
        let width = ((area.width + 1) / self.card_capacity as u16).saturating_sub(1).min(38);
        let occupied = range.len() as u16 * (width + 1) - 1;
        let start_x = area.right() - occupied;
        for (slot, position) in range.enumerate() {
            let month = &self.history.months[indices[position]];
            let records = month.invoices(self.invoice_type);
            let selected = position == self.selected_month;
            let record_index = if selected { self.selected_invoice.min(records.len() - 1) } else { 0 };
            let invoice = &records[record_index];
            let card = Rect::new(start_x + slot as u16 * (width + 1), area.y, width, area.height);
            let title = format!("{}{}{}", if position == 0 { "" } else if slot == 0 { "< " } else { "" },
                month.month.format("%b %Y"), if position + 1 < indices.len() && slot + 1 == self.card_capacity { " >" } else { "" });
            let inner = pane(frame, card, &title, selected && self.focus == Focus::Cards, Some(self.invoice_color()));
            let body = &invoice["InvoiceBody"];
            let number = body["InvoiceNumber"].as_str().unwrap_or("-");
            let party_key = if self.invoice_type == InvoiceType::Sales { "Buyer" } else { "Seller" };
            let party = invoice[party_key]["IdentificationData"]["Name"].as_str().unwrap_or("-");
            let currency = body["CurrencyCode"].as_str().unwrap_or("-");
            let gross = body["TotalGrossAmount"].as_f64().unwrap_or_default();
            let mut lines = vec![number.to_string(), party.to_string(), format!("{gross:.2} {currency}")];
            if inner.height >= 6 {
                lines.push(format!("Issued: {}", body["IssueDate"].as_str().unwrap_or("-")));
                lines.push(format!("{} invoice(s) this month", records.len()));
            }
            let body_area = Rect::new(inner.x, inner.y, inner.width, inner.height.saturating_sub(1));
            frame.render_widget(Paragraph::new(lines.into_iter().map(|line| Line::from(widgets::clipped_line(&line, inner.width))).collect::<Vec<_>>()), body_area);
            let label = format!("[Download] (d) {}/{}", record_index + 1, records.len());
            frame.render_widget(Paragraph::new(widgets::clipped_line(&label, inner.width))
                .style(if selected { bold() } else { Style::default().add_modifier(Modifier::DIM) }),
                Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1));
        }
    }

    fn invoice_color(&self) -> Color {
        match self.invoice_type {
            InvoiceType::Sales => Color::Blue,
            InvoiceType::Purchases => Color::Rgb(255, 165, 0),
        }
    }

    fn draw_customers(&mut self, frame: &mut Frame, area: Rect) {
        let title = if self.customer_details { "💼 Customer details" } else { "💼 Customers" };
        let inner = pane(frame, area, title, self.section == Section::Customers, None);
        let parts = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(inner);
        if let LoadState::Failed(error) = &self.customer_state {
            self.customer_height = draw_text(frame, parts[0], &[format!("Load failed: {error}\nr: retry")], &mut self.customer_scroll);
        } else if matches!(self.customer_state, LoadState::Pending) {
            frame.render_widget(Paragraph::new("Loading customers..."), parts[0]);
        } else if self.customer_details {
            let lines = self.customer().map(customers::preview).unwrap_or_else(|| vec!["No customer selected.".to_string()]);
            self.customer_height = draw_text(frame, parts[0], &lines, &mut self.customer_scroll);
        } else if self.customers.is_empty() {
            frame.render_widget(Paragraph::new("No customers found."), parts[0]);
        } else {
            let labels: Vec<_> = self.customers.iter().map(|customer| customer.name.clone()).collect();
            draw_list(frame, parts[0], &labels, &mut self.customer_selection);
        }
        let mut buttons = vec!["New (n)"];
        if self.customer_details {
            if self.customer().is_some() { buttons.push("Edit (e)"); }
            buttons.push("Back (Esc)");
        }
        draw_buttons(frame, parts[1], &buttons);
    }

    fn draw_settings(&mut self, frame: &mut Frame, area: Rect) {
        let title = format!("{}🧑‍💻 User details{}", if self.focus == Focus::Settings { widgets::SELECTOR } else { "" },
            if self.user == self.draft { "" } else { " * unsaved" });
        let inner = pane(frame, area, &title, self.section == Section::Settings, None);
        let parts = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(inner);
        draw_list(frame, parts[0], &users::fields(&self.draft), &mut self.settings_selection);
        if self.user != self.draft {
            draw_buttons(frame, parts[1], &["Save (w)", "Discard (x)"]);
        } else {
            frame.render_widget(Paragraph::new("Enter: edit field"), parts[1]);
        }
    }
}

fn bold() -> Style { Style::default().add_modifier(Modifier::BOLD) }

fn pane(frame: &mut Frame, area: Rect, title: &str, focused: bool, color: Option<Color>) -> Rect {
    let block = widgets::block().title(Line::from(format!(" {title} ")).style(if focused { bold() } else { Style::default() }))
        .padding(Padding::new(1, 1, 1, 0))
        .border_type(if focused && color.is_some() { BorderType::Thick } else { BorderType::Plain })
        .border_style(if let Some(color) = color { Style::default().fg(color) } else if focused { bold() } else { Style::default() });
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
    let lines = widgets::wrap_lines(&lines.join("\n"), area.width);
    *offset = (*offset).min(lines.len().saturating_sub(area.height as usize).min(u16::MAX as usize) as u16);
    frame.render_widget(Paragraph::new(lines.into_iter().map(Line::from).collect::<Vec<_>>()).scroll((*offset, 0)), area);
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
            app.customer_state = LoadState::Pending;
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
            KeyCode::Char('s') => app.set_type(InvoiceType::Sales),
            KeyCode::Char('c') => app.set_type(InvoiceType::Purchases),
            KeyCode::Tab => app.cycle_focus(false),
            KeyCode::BackTab => app.cycle_focus(true),
            KeyCode::Char('n') if app.section == Section::Invoices && app.invoice_type == InvoiceType::Sales => activate(&mut app, terminal, Action::NewInvoice).await?,
            KeyCode::Char('n') if app.focus == Focus::Customers => activate(&mut app, terminal, Action::NewCustomer).await?,
            KeyCode::Char('e') if app.focus == Focus::Customers && app.customer_details => activate(&mut app, terminal, Action::EditCustomer).await?,
            KeyCode::Char('w') if app.focus == Focus::Settings && app.user != app.draft => activate(&mut app, terminal, Action::Save).await?,
            KeyCode::Char('x') if app.focus == Focus::Settings && app.user != app.draft => activate(&mut app, terminal, Action::Discard).await?,
            KeyCode::Char('d') if app.section == Section::Invoices => activate(&mut app, terminal, Action::Download).await?,
            KeyCode::Char('r') => {
                if matches!(app.history_state, LoadState::Failed(_)) { app.history_state = LoadState::Pending; }
                if matches!(app.customer_state, LoadState::Failed(_)) { app.customer_state = LoadState::Pending; }
            }
            KeyCode::Esc if app.section == Section::Customers && app.customer_details => {
                activate(&mut app, terminal, Action::Back).await?;
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                if app.user == app.draft || widgets::confirm(terminal, "Discard unsaved settings and quit?", false)? { return Ok(()); }
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};
    use serde_json::json;

    fn app() -> Workspace {
        let user: AppUser = serde_json::from_value(json!({"id": "123", "email": "user@example.com", "tier": 0})).unwrap();
        let mut app = Workspace::new(&user);
        let invoices = [1, 3, 8, 9].into_iter().map(|month| json!({
            "Buyer": {"IdentificationData": {"Name": "Example company"}},
            "InvoiceBody": {
                "IssueDate": format!("2026-{month:02}-01"), "InvoiceNumber": format!("FV/{month}"),
                "CurrencyCode": "PLN", "TotalGrossAmount": 123.0,
            }
        })).collect();
        app.history = history::from_rows(
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(), NaiveDate::from_ymd_opt(2026, 10, 2).unwrap(), invoices, vec![],
        ).unwrap();
        app.history_state = LoadState::Ready;
        app.customer_state = LoadState::Ready;
        app.restore_month(None);
        app
    }

    fn render(app: &mut Workspace, width: u16, height: u16) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        terminal.backend().buffer().clone()
    }

    fn lines(buffer: &Buffer) -> Vec<String> {
        buffer.content.chunks(buffer.area.width as usize)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect()).collect()
    }

    #[test]
    fn dashboard_has_no_header_and_exactly_one_footer_row() {
        let mut app = app();
        for (width, height) in [(48, 24), (80, 24), (100, 40), (160, 50)] {
            for focus in [Focus::Cards, Focus::Customers, Focus::Settings] {
                app.set_focus(focus);
                let buffer = render(&mut app, width, height);
                let rows = lines(&buffer);
                let text = rows.join("\n");
                assert!(!text.contains("KSeF Bot") && !text.contains("1. Invoices"));
                assert!(rows[1].contains("(s)") && rows[1].contains("Sales"));
                assert!(rows[1].contains("(c)") && rows[1].contains("Purchases"));
                assert!(!text.contains("Gross PLN") && !text.contains("Tab:focus") && !text.contains("1/2/3"));
                assert!(!rows[0].contains("[New]") && !text.contains("[Older]"));
                assert_eq!(rows.iter().filter(|row| row.contains("s/c:invoice type")).count(), 1);
                assert!(rows.last().unwrap().contains("s/c:invoice type"));
                assert!(rows.last().unwrap().contains("Tab:next") || rows.last().unwrap().contains("Tab/Shift+Tab"));
                assert!(rows.last().unwrap().contains("q:quit"));
                assert!(Line::from(app.footer(width)).width() <= width as usize);
                assert!(text.contains("Customers") && text.contains("User details"));
            }
        }
    }

    #[test]
    fn invoice_cards_are_flat_with_details_and_downloads() {
        let mut app = app();
        for (width, height) in [(48, 24), (80, 24), (100, 40)] {
            let text = lines(&render(&mut app, width, height)).join("\n");
            assert!(!text.contains('╱') && !text.contains('░') && !text.contains('█'));
            assert!(text.contains('┌') && text.contains('┐'));
            assert!(text.contains("Jan 2026"));
            assert!(text.contains("FV/1"));
            assert!(text.contains("123.00 PLN"));
            assert!(text.contains("[Download]"));
            assert!(!text.contains("Feb 2026"));
        }
    }

    #[test]
    fn lone_card_is_right_aligned_and_stays_within_its_area() {
        let mut app = app();
        app.history.months.drain(..3);
        app.restore_month(None);
        let buffer = render(&mut app, 100, 40);
        let rows = lines(&buffer);
        let top = rows.iter().position(|row| row.contains('┏') && row.contains('┓')).unwrap();
        let left = (0..100).find(|x| buffer[(*x, top as u16)].symbol() == "┏").unwrap();
        let right = (0..100).rfind(|x| buffer[(*x, top as u16)].symbol() == "┓").unwrap();
        assert!(left >= 60);
        assert_eq!(right, 98);
        assert!((0..40).all(|y| buffer[(99, y)].symbol() == " "));
    }

    #[test]
    fn focus_cycles_through_every_card_then_customers_and_settings() {
        let mut app = app();
        assert_eq!(app.focus, Focus::Cards);
        assert_eq!(app.selected_month, 0);
        for month in 1..4 {
            app.selected_invoice = 1;
            app.cycle_focus(false);
            assert_eq!(app.focus, Focus::Cards);
            assert_eq!(app.selected_month, month);
            assert_eq!(app.selected_invoice, 0);
            let buffer = render(&mut app, 48, 24);
            assert!(lines(&buffer)[4].contains(&app.selected_date().unwrap().format("%b %Y").to_string()));
        }
        app.cycle_focus(false);
        assert_eq!(app.focus, Focus::Customers);
        app.cycle_focus(false);
        assert_eq!(app.focus, Focus::Settings);
        app.cycle_focus(false);
        assert_eq!(app.focus, Focus::Cards);
        assert_eq!(app.selected_month, 0);
        app.cycle_focus(true);
        assert_eq!(app.focus, Focus::Settings);
        app.cycle_focus(true);
        assert_eq!(app.focus, Focus::Customers);
        for month in (0..4).rev() {
            app.cycle_focus(true);
            assert_eq!(app.focus, Focus::Cards);
            assert_eq!(app.selected_month, month);
        }
    }

    #[test]
    fn card_selection_scrolls_to_older_nonempty_months() {
        let mut app = app();
        assert_eq!(app.selected_date().unwrap().month(), 1);
        app.card_range(78);
        assert_eq!(app.card_capacity, 2);
        app.move_card(KeyCode::Home);
        let text = lines(&render(&mut app, 80, 24)).join("\n");
        assert!(text.contains("Jan 2026") && text.contains("Mar 2026"));
        assert!(!text.contains("Sep 2026"));
        app.move_card(KeyCode::End);
        assert!(lines(&render(&mut app, 80, 24)).join("\n").contains("Sep 2026"));
    }

    #[test]
    fn empty_or_failed_history_never_invents_invoice_cards() {
        let mut app = app();
        app.history.months.clear();
        for state in [LoadState::Ready, LoadState::Failed("Backend rejected request".to_string())] {
            app.history_state = state;
            let text = lines(&render(&mut app, 100, 40)).join("\n");
            assert!(!text.contains("╱│") && !text.contains("[Download]"));
            assert!(text.contains("Customers"));
            assert!(text.contains("No sales invoices") || text.contains("Backend rejected request"));
        }
    }

    #[test]
    fn scrolling_past_oldest_card_requests_earlier_history_once() {
        let mut app = app();
        app.move_card(KeyCode::Home);
        assert!(matches!(app.history_state, LoadState::Ready));
        app.move_card(KeyCode::Left);
        assert!(app.older);
        assert!(matches!(app.history_state, LoadState::Pending));
        assert_eq!(app.selected_month, 0);
        app.move_card(KeyCode::Left);
        assert_eq!(app.selected_month, 0);
        app.history_state = LoadState::Failed("Request failed".to_string());
        app.move_card(KeyCode::PageUp);
        assert!(matches!(app.history_state, LoadState::Failed(_)));
    }

    #[test]
    fn empty_history_can_be_focused_and_scrolled_to_search_earlier_months() {
        let mut app = app();
        app.history.months.clear();
        app.restore_month(None);
        app.set_focus(Focus::Settings);
        app.cycle_focus(false);
        assert_eq!(app.focus, Focus::Cards);
        app.move_card(KeyCode::Left);
        assert!(app.older && matches!(app.history_state, LoadState::Pending));
    }

    #[test]
    fn choosing_invoice_type_returns_focus_to_cards_even_when_type_is_unchanged() {
        let mut app = app();
        app.set_focus(Focus::Customers);
        app.set_type(InvoiceType::Sales);
        assert_eq!(app.focus, Focus::Cards);
        assert_eq!(app.section, Section::Invoices);
        app.set_focus(Focus::Settings);
        app.set_type(InvoiceType::Purchases);
        assert_eq!(app.invoice_type, InvoiceType::Purchases);
        assert_eq!(app.focus, Focus::Cards);
        assert_eq!(app.section, Section::Invoices);
    }

    #[test]
    fn type_selector_highlights_three_rows_and_only_shows_the_selected_emoji() {
        let mut app = app();
        for kind in [InvoiceType::Sales, InvoiceType::Purchases] {
            app.set_type(kind);
            for width in [48, 80, 120] {
                let buffer = render(&mut app, width, 24);
                let rows = lines(&buffer);
                assert!(rows[0].trim().is_empty());
                assert!(rows[2].trim().is_empty());
                assert!(rows[3].trim().is_empty());
                let (label, emoji, hidden) = if kind == InvoiceType::Sales { ("Sales", "💵", "🛒") } else { ("Purchases", "🛒", "💵") };
                assert!(rows[1].find(label).unwrap() < rows[1].find(emoji).unwrap());
                assert!(!rows[1].contains(hidden));
                let split = 1 + Line::from(" (s) Sales 💵 ").width() as u16;
                let end = split + Line::from(" (c) Purchases 🛒 ").width() as u16;
                for x in 1..width - 1 {
                    let selected = if kind == InvoiceType::Sales { x < split } else { (split..end).contains(&x) };
                    assert_eq!(buffer[(x, 0)].modifier.contains(Modifier::REVERSED), selected);
                    assert_eq!(buffer[(x, 2)].modifier.contains(Modifier::REVERSED), selected);
                    if Line::from(buffer[(x - 1, 1)].symbol()).width() < 2 {
                        assert_eq!(buffer[(x, 1)].modifier.contains(Modifier::REVERSED), selected);
                    }
                    assert!(!buffer[(x, 3)].modifier.contains(Modifier::REVERSED));
                }
            }
        }
    }

    #[test]
    fn invoice_borders_use_type_colors_and_only_the_focused_card_is_thick() {
        let mut app = app();
        for month in &mut app.history.months { month.purchases = month.sales.clone(); }
        for (kind, color) in [(InvoiceType::Sales, Color::Blue), (InvoiceType::Purchases, Color::Rgb(255, 165, 0))] {
            app.set_type(kind);
            let buffer = render(&mut app, 160, 30);
            let corners: Vec<_> = (0..160).filter(|x| matches!(buffer[(*x, 4)].symbol(), "┏" | "┌")).collect();
            assert_eq!(corners.len(), 4);
            for (index, x) in corners.iter().enumerate() {
                assert_eq!(buffer[(*x, 4)].symbol(), if index == 0 { "┏" } else { "┌" });
                assert_eq!(buffer[(*x, 4)].fg, color);
                assert_eq!(buffer[(*x + 2, 4)].modifier.contains(Modifier::BOLD), index == 0);
                assert_eq!(buffer[(*x + 2, 5)].symbol(), " ");
                assert_eq!(buffer[(*x + 2, 6)].symbol(), "F");
            }
            app.set_focus(Focus::Customers);
            let buffer = render(&mut app, 160, 30);
            assert!(!lines(&buffer)[4].contains('┏'));
        }
    }

    #[test]
    fn lower_panes_have_matching_emojis_and_top_padding() {
        let mut app = app();
        for focus in [Focus::Customers, Focus::Settings] {
            app.set_focus(focus);
            let rows = lines(&render(&mut app, 100, 30));
            let top = rows.iter().position(|row| row.contains("Customers")).unwrap();
            assert!(rows[top].contains("💼") && rows[top].contains("🧑‍💻"));
            assert!(!rows[top].contains("❯ 💼"));
            assert!(rows[top + 1].chars().all(|ch| ch == ' ' || ch == '│'));
            assert!(rows[top + 2].contains("No customers found."));
            assert_eq!(rows[top].contains('❯'), focus == Focus::Settings);
        }
    }

    #[test]
    fn card_preview() {
        println!("{}", lines(&render(&mut app(), 100, 30)).join("\n"));
    }
}