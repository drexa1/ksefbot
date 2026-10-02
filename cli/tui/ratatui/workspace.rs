use super::{Tui, customers, history::{self, InvoiceHistory}, invoices::{self, InvoiceType}, month_range, read_key, users, widgets};
use crate::api::{customers::AppContractor, users::AppUser};
use anyhow::Result;
use chrono::{Datelike, Local, Months, NaiveDate};
use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState, Padding, Paragraph},
};
use serde_json::Value;
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Section { Invoices, Customers, Settings }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Focus { Type, Cards, Customers, Settings, Action(Action) }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action { NewInvoice, Download, Older, NewCustomer, EditCustomer, Back, Save, Discard }

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
    chart_currency: usize,
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
            card_start: 0, card_capacity: 1, chart_currency: 0,
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

    fn focus_order(&self) -> Vec<Focus> {
        let mut order = vec![Focus::Type, Focus::Action(Action::Older)];
        if self.invoice_type == InvoiceType::Sales { order.push(Focus::Action(Action::NewInvoice)); }
        if self.invoice().is_some() { order.extend([Focus::Cards, Focus::Action(Action::Download)]); }
        order.extend([Focus::Customers, Focus::Action(Action::NewCustomer)]);
        if self.customer_details {
            if self.customer().is_some() { order.push(Focus::Action(Action::EditCustomer)); }
            order.push(Focus::Action(Action::Back));
        }
        order.push(Focus::Settings);
        if self.user != self.draft { order.extend([Focus::Action(Action::Save), Focus::Action(Action::Discard)]); }
        order
    }

    fn set_focus(&mut self, focus: Focus) {
        self.focus = focus;
        self.section = match focus {
            Focus::Customers | Focus::Action(Action::NewCustomer | Action::EditCustomer | Action::Back) => Section::Customers,
            Focus::Settings | Focus::Action(Action::Save | Action::Discard) => Section::Settings,
            _ => Section::Invoices,
        };
    }

    fn cycle_focus(&mut self, backwards: bool) {
        let order = self.focus_order();
        let index = order.iter().position(|focus| *focus == self.focus).unwrap_or_default();
        self.set_focus(order[(index + if backwards { order.len() - 1 } else { 1 }) % order.len()]);
    }

    fn switch_section(&mut self, section: Section) {
        self.section = section;
        self.focus = match section {
            Section::Invoices => Focus::Cards,
            Section::Customers => Focus::Customers,
            Section::Settings => Focus::Settings,
        };
    }

    fn set_type(&mut self, kind: InvoiceType) {
        if self.invoice_type == kind { return; }
        let date = self.selected_date();
        self.invoice_type = kind;
        self.restore_month(date);
    }

    fn restore_month(&mut self, date: Option<NaiveDate>) {
        let indices = self.month_indices();
        self.selected_month = indices.iter().position(|index| Some(self.history.months[*index].month) == date)
            .unwrap_or_else(|| indices.len().saturating_sub(1));
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
                    "No {} invoices from {} to {}. Choose Older again to continue searching.",
                    self.invoice_type, self.history.start, previous_start.pred_opt().unwrap(),
                )])?;
            }
        }
        if !self.focus_order().contains(&self.focus) { self.set_focus(Focus::Type); }
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
            Constraint::Length(((screen.height - 1) / 2).max(14)),
            Constraint::Min(7), Constraint::Length(1),
        ]).split(area);
        let top = Layout::vertical([Constraint::Min(4), Constraint::Length(1), Constraint::Length(if screen.height >= 30 { 10 } else { 9 })]).split(sections[0]);
        self.draw_chart(frame, top[0]);
        self.draw_type(frame, top[1]);
        self.draw_cards(frame, top[2]);
        let lower = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).spacing(1).split(sections[1]);
        self.draw_customers(frame, lower[0]);
        self.draw_settings(frame, lower[1]);
        frame.render_widget(Paragraph::new(self.footer(sections[2].width)), sections[2]);
    }

    fn footer(&self, width: u16) -> String {
        let hint = match self.focus {
            Focus::Cards | Focus::Action(Action::Download) => "Left/Right:months Up/Down:invoice d:download",
            Focus::Type => "Left/Right:type o:older c:currency",
            Focus::Customers if self.customer_details => "Up/Down:scroll Esc:list",
            Focus::Customers => "Up/Down:customer Enter:details",
            Focus::Settings => "Up/Down:field Enter:edit",
            _ => "Enter:activate Esc:back",
        };
        let global = if width >= 80 { "Tab:focus 1/2/3:jump q:quit" } else { "Tab:focus q:quit" };
        widgets::clipped_line(&format!("{global} | {hint}"), width)
    }

    fn draw_type(&self, frame: &mut Frame, area: Rect) {
        let rows = Layout::horizontal([Constraint::Min(25), Constraint::Length(if self.invoice_type == InvoiceType::Sales { 19 } else { 9 })]).split(area);
        let spans = [(InvoiceType::Sales, " 💵 Sales "), (InvoiceType::Purchases, " 🛒 Purchases ")].into_iter()
            .map(|(kind, text)| Span::styled(text, if self.invoice_type == kind { Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD) } else { Style::default() }))
            .collect::<Vec<_>>();
        frame.render_widget(Paragraph::new(Line::from(spans)), rows[0]);
        let mut buttons = vec![(Action::Older, "Older")];
        if self.invoice_type == InvoiceType::Sales { buttons.push((Action::NewInvoice, "New")); }
        draw_buttons(frame, rows[1], &buttons, self.focus);
    }

    fn draw_cards(&mut self, frame: &mut Frame, area: Rect) {
        let indices = self.month_indices();
        let range = self.card_range(area.width);
        if range.is_empty() {
            let title = format!(" {} invoices ", self.invoice_type);
            let inner = pane(frame, area, &title, self.focus == Focus::Cards);
            let status = match &self.history_state {
                LoadState::Pending => "Loading invoice history...".to_string(),
                LoadState::Failed(error) => format!("Load failed: {error}\nPress r to retry."),
                LoadState::Ready => format!("No {} invoices from {} to {}.\nChoose Older to search earlier months.", self.invoice_type, self.history.start, self.history.end),
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
            let inner = cube(frame, card, &title, selected);
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
            let download = if selected && self.focus == Focus::Action(Action::Download) { "❯ Download" } else { "Download" };
            let label = format!("[{download}] {}/{}", record_index + 1, records.len());
            frame.render_widget(Paragraph::new(widgets::clipped_line(&label, inner.width))
                .style(if selected { bold() } else { Style::default().add_modifier(Modifier::DIM) }),
                Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1));
        }
    }

    fn draw_chart(&mut self, frame: &mut Frame, area: Rect) {
        let totals: Result<Vec<_>> = self.history.months.iter().map(|month| month.totals()).collect();
        let totals = match totals {
            Ok(totals) => totals,
            Err(error) => {
                let inner = pane(frame, area, "Monthly amounts", false);
                draw_text(frame, inner, &[format!("Cannot chart amounts: {error:#}")], &mut 0);
                return;
            }
        };
        let currencies: std::collections::BTreeSet<_> = totals.iter().flatten().map(|total| total.currency.as_str()).collect();
        self.chart_currency %= currencies.len().max(1);
        let currency = currencies.iter().nth(self.chart_currency).copied().unwrap_or("PLN");
        let status = match &self.history_state {
            LoadState::Pending => " | loading...",
            LoadState::Failed(_) => " | load failed: r retry",
            _ => "",
        };
        let selected_totals = self.selected_date()
            .and_then(|date| self.history.months.iter().position(|month| month.month == date))
            .and_then(|index| totals[index].iter().find(|total| total.currency == currency));
        let title = if area.width >= 80 {
            match selected_totals {
                Some(total) => format!("Gross {currency} | █ Sales {:.2}  ░ Expenses {:.2} | {}{status}",
                    total.sales, total.purchases, self.selected_date().unwrap().format("%b %Y")),
                None => format!("Gross {currency} | █ Sales ░ Expenses{status}"),
            }
        } else { format!("Gross {currency} | █ Sales ░ Expenses{status}") };
        frame.render_widget(Paragraph::new(widgets::clipped_line(&title, area.width)).style(bold()), Rect::new(area.x, area.y, area.width, 1));
        let inner = Rect::new(area.x, area.y + 1, area.width, area.height.saturating_sub(1));
        if self.history.months.is_empty() {
            draw_text(frame, inner, &[format!("History: {} - {} | o: older", self.history.start, self.history.end)], &mut 0);
            return;
        }
        let count = (inner.width / 10).max(1) as usize;
        let end = self.selected_date().and_then(|date| self.history.months.iter().position(|month| month.month == date))
            .map_or(self.history.months.len(), |index| (index + 1).max(count).min(self.history.months.len()));
        let start = end.saturating_sub(count);
        let data: Vec<_> = (start..end).map(|index| {
            let total = totals[index].iter().find(|total| total.currency == currency);
            (self.history.months[index].month, total.map_or(0.0, |total| total.sales), total.map_or(0.0, |total| total.purchases))
        }).collect();
        draw_stacks(frame, inner, &data);
    }

    fn draw_customers(&mut self, frame: &mut Frame, area: Rect) {
        let title = if self.customer_details { "Customer details" } else { "Customers" };
        let inner = pane(frame, area, title, self.section == Section::Customers);
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
        let mut buttons = vec![(Action::NewCustomer, "New")];
        if self.customer_details {
            if self.customer().is_some() { buttons.push((Action::EditCustomer, "Edit")); }
            buttons.push((Action::Back, "Back"));
        }
        draw_buttons(frame, parts[1], &buttons, self.focus);
    }

    fn draw_settings(&mut self, frame: &mut Frame, area: Rect) {
        let title = if self.user == self.draft { "User details" } else { "User details * unsaved" };
        let inner = pane(frame, area, title, self.section == Section::Settings);
        let parts = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(inner);
        draw_list(frame, parts[0], &users::fields(&self.draft), &mut self.settings_selection);
        if self.user != self.draft {
            draw_buttons(frame, parts[1], &[(Action::Save, "Save"), (Action::Discard, "Discard")], self.focus);
        } else {
            frame.render_widget(Paragraph::new("Enter: edit field"), parts[1]);
        }
    }
}

fn bold() -> Style { Style::default().add_modifier(Modifier::BOLD) }

fn pane(frame: &mut Frame, area: Rect, title: &str, focused: bool) -> Rect {
    let block = widgets::block().title(format!(" {}{title} ", if focused { widgets::SELECTOR } else { "" }))
        .padding(Padding::horizontal(1)).border_style(if focused { bold() } else { Style::default() });
    let inner = block.inner(area);
    frame.render_widget(block, area);
    inner
}

fn cube(frame: &mut Frame, area: Rect, title: &str, selected: bool) -> Rect {
    let front = Rect::new(area.x, area.y + 2, area.width.saturating_sub(2), area.height.saturating_sub(2));
    let edge = if selected { bold() } else { Style::default().add_modifier(Modifier::DIM) };
    frame.render_widget(Paragraph::new(format!("┌{}┐", "─".repeat(front.width.saturating_sub(2) as usize))).style(edge),
        Rect::new(area.x + 2, area.y, front.width, 1));
    frame.render_widget(Paragraph::new(format!("╱{}╱│", "░".repeat(front.width.saturating_sub(2) as usize))).style(edge),
        Rect::new(area.x + 1, area.y + 1, front.width + 1, 1));
    for y in area.y + 2..area.bottom().saturating_sub(2) {
        frame.render_widget(Paragraph::new("░│").style(edge), Rect::new(front.right(), y, 2, 1));
    }
    frame.render_widget(Paragraph::new("╱").style(edge), Rect::new(front.right(), area.bottom().saturating_sub(2), 1, 1));
    pane(frame, front, title, selected)
}

fn draw_list(frame: &mut Frame, area: Rect, labels: &[String], state: &mut ListState) {
    let items: Vec<_> = labels.iter().map(|label| ListItem::new(widgets::clipped_line(label, area.width.saturating_sub(2)))).collect();
    frame.render_stateful_widget(List::new(items).highlight_symbol(widgets::SELECTOR), area, state);
}

fn draw_buttons(frame: &mut Frame, area: Rect, buttons: &[(Action, &str)], focus: Focus) {
    let columns = Layout::horizontal(vec![Constraint::Ratio(1, buttons.len() as u32); buttons.len()]).split(area);
    for ((action, label), column) in buttons.iter().zip(columns.iter()) {
        let active = focus == Focus::Action(*action);
        frame.render_widget(Paragraph::new(format!("{}[{label}]", if active { widgets::SELECTOR } else { "" }))
            .alignment(Alignment::Center).style(if active { bold() } else { Style::default() }), *column);
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

fn stack_heights(sales: f64, expenses: f64, maximum: f64, rows: u16) -> (u16, u16) {
    let total = sales + expenses;
    if maximum <= 0.0 || total <= 0.0 { return (0, 0); }
    let height = ((total / maximum * f64::from(rows)).round() as u16).min(rows);
    let sales_height = (sales / total * f64::from(height)).round() as u16;
    (sales_height.min(height), height.saturating_sub(sales_height))
}

fn draw_stacks(frame: &mut Frame, area: Rect, data: &[(NaiveDate, f64, f64)]) {
    if area.height < 2 || data.is_empty() { return; }
    let positive = data.iter().map(|(_, sales, expenses)| sales.max(0.0) + expenses.max(0.0)).fold(0.0, f64::max);
    let negative = data.iter().map(|(_, sales, expenses)| (-sales).max(0.0) + (-expenses).max(0.0)).fold(0.0, f64::max);
    let baseline_rows = u16::from(positive > 0.0 && negative > 0.0 && area.height >= 4);
    let plot_height = area.height - 1 - baseline_rows;
    let negative_rows = if negative > 0.0 {
        ((negative / (positive + negative) * f64::from(plot_height)).round() as u16).max(1).min(plot_height)
    } else { 0 };
    let positive_rows = plot_height - negative_rows;
    let scale = if positive_rows == 0 { negative / f64::from(negative_rows.max(1)) }
        else if negative_rows == 0 { positive / f64::from(positive_rows) }
        else { (positive / f64::from(positive_rows)).max(negative / f64::from(negative_rows)) };
    let width = (area.width / data.len() as u16).max(1);
    if baseline_rows > 0 {
        frame.render_widget(Paragraph::new(format!("0{}", "─".repeat(area.width.saturating_sub(1) as usize))),
            Rect::new(area.x, area.y + positive_rows, area.width, 1));
    }
    for (index, (month, sales, expenses)) in data.iter().enumerate() {
        let x = area.x + index as u16 * width;
        let bar_width = width.saturating_sub(2).clamp(1, 7);
        let bar_x = x + (width - bar_width) / 2;
        let (sale_rows, expense_rows) = stack_heights(sales.max(0.0), expenses.max(0.0), scale * f64::from(positive_rows), positive_rows);
        for row in 0..sale_rows + expense_rows {
            let y = area.y + positive_rows - 1 - row;
            frame.render_widget(Paragraph::new(if row < sale_rows { "█" } else { "░" }.repeat(bar_width as usize)), Rect::new(bar_x, y, bar_width, 1));
        }
        let (sale_rows, expense_rows) = stack_heights((-sales).max(0.0), (-expenses).max(0.0), scale * f64::from(negative_rows), negative_rows);
        for row in 0..sale_rows + expense_rows {
            let y = area.y + positive_rows + baseline_rows + row;
            frame.render_widget(Paragraph::new(if row < sale_rows { "█" } else { "░" }.repeat(bar_width as usize)), Rect::new(bar_x, y, bar_width, 1));
        }
        frame.render_widget(Paragraph::new(month.format("%b %y").to_string()).alignment(Alignment::Center), Rect::new(x, area.bottom() - 1, width, 1));
    }
}

async fn activate(app: &mut Workspace, terminal: &mut Tui, action: Action) -> Result<()> {
    let result = match action {
        Action::Older => { app.older = true; app.history_state = LoadState::Pending; return Ok(()); }
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
            KeyCode::Char('1') => app.switch_section(Section::Invoices),
            KeyCode::Char('2') => app.switch_section(Section::Customers),
            KeyCode::Char('3') => app.switch_section(Section::Settings),
            KeyCode::Tab => app.cycle_focus(false),
            KeyCode::BackTab => app.cycle_focus(true),
            KeyCode::Char('c') => app.chart_currency = app.chart_currency.saturating_add(1),
            KeyCode::Char('o') => activate(&mut app, terminal, Action::Older).await?,
            KeyCode::Char('d') if app.section == Section::Invoices => activate(&mut app, terminal, Action::Download).await?,
            KeyCode::Char('r') => {
                if matches!(app.history_state, LoadState::Failed(_)) { app.history_state = LoadState::Pending; }
                if matches!(app.customer_state, LoadState::Failed(_)) { app.customer_state = LoadState::Pending; }
            }
            KeyCode::Esc if app.section == Section::Customers && app.customer_details => {
                app.customer_details = false;
                app.set_focus(Focus::Customers);
            }
            KeyCode::Esc if matches!(app.focus, Focus::Action(_)) => app.switch_section(app.section),
            KeyCode::Esc | KeyCode::Char('q') => {
                if app.user == app.draft || widgets::confirm(terminal, "Discard unsaved settings and quit?", false)? { return Ok(()); }
            }
            KeyCode::Left | KeyCode::Right | KeyCode::Enter if app.focus == Focus::Type => {
                app.set_type(match app.invoice_type { InvoiceType::Sales => InvoiceType::Purchases, InvoiceType::Purchases => InvoiceType::Sales });
            }
            KeyCode::Enter => match app.focus {
                Focus::Cards => if let Some(invoice) = app.invoice() {
                    widgets::message(terminal, "Invoice details", &invoices::invoice_preview(invoice, &app.invoice_type))?;
                },
                Focus::Customers => app.open_customer(),
                Focus::Settings => {
                    if let Some(index) = app.settings_selection.selected() { users::edit_field(terminal, &mut app.draft, index)?; }
                }
                Focus::Action(action) => activate(&mut app, terminal, action).await?,
                _ => {}
            },
            _ if matches!(app.focus, Focus::Cards | Focus::Action(Action::Download)) => app.move_card(key),
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
            for focus in [Focus::Type, Focus::Cards, Focus::Customers, Focus::Settings, Focus::Action(Action::Older)] {
                app.set_focus(focus);
                let buffer = render(&mut app, width, height);
                let rows = lines(&buffer);
                let text = rows.join("\n");
                assert!(!text.contains("KSeF Bot") && !text.contains("1. Invoices"));
                assert!(rows[0].starts_with(" Gross PLN"));
                assert_eq!(rows.iter().filter(|row| row.contains("Tab:focus")).count(), 1);
                assert!(rows.last().unwrap().contains("Tab:focus"));
                assert!(rows.last().unwrap().contains("q:quit"));
                assert!(Line::from(app.footer(width)).width() <= width as usize);
                assert!(text.contains("Customers") && text.contains("User details"));
            }
        }
    }

    #[test]
    fn invoice_cubes_have_raised_tops_side_faces_details_and_downloads() {
        let mut app = app();
        for (width, height) in [(48, 24), (80, 24), (100, 40)] {
            let text = lines(&render(&mut app, width, height)).join("\n");
            assert!(text.contains("╱░") && text.contains("╱│") && text.contains("░│"));
            assert!(text.contains("Sep 2026"));
            assert!(text.contains("FV/9"));
            assert!(text.contains("123.00 PLN"));
            assert!(text.contains("[Download]"));
            assert!(!text.contains("Feb 2026"));
        }
    }

    #[test]
    fn lone_cube_is_right_aligned_and_stays_within_its_area() {
        let mut app = app();
        app.history.months.drain(..3);
        app.restore_month(None);
        let buffer = render(&mut app, 100, 40);
        let rows = lines(&buffer);
        let top = rows.iter().position(|row| row.contains('┌') && row.contains('┐')).unwrap();
        let left = (0..100).find(|x| buffer[(*x, top as u16)].symbol() == "┌").unwrap();
        let right = (0..100).rfind(|x| buffer[(*x, top as u16)].symbol() == "┐").unwrap();
        assert!(left >= 60);
        assert_eq!(right, 98);
        assert!((0..40).all(|y| buffer[(99, y)].symbol() == " "));
    }

    #[test]
    fn focus_cycle_has_no_removed_header_stop() {
        let mut app = app();
        let order = app.focus_order();
        assert_eq!(order[0], Focus::Type);
        app.set_focus(Focus::Type);
        for expected in order.iter().skip(1) {
            app.cycle_focus(false);
            assert_eq!(app.focus, *expected);
        }
        app.cycle_focus(false);
        assert_eq!(app.focus, Focus::Type);
    }

    #[test]
    fn cube_selection_scrolls_to_older_nonempty_months() {
        let mut app = app();
        assert_eq!(app.selected_date().unwrap().month(), 9);
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
    fn empty_or_failed_history_never_invents_invoice_cubes() {
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
    fn cube_preview() {
        println!("{}", lines(&render(&mut app(), 100, 30)).join("\n"));
    }
}