use super::{Tui, read_key};
use anyhow::Result;
use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Clear, List, ListItem, ListState, Padding, Paragraph, Wrap},
};

pub const SELECTOR: &str = "❯ ";
pub const MIN_WIDTH: u16 = 48;
pub const MIN_HEIGHT: u16 = 24;

pub fn highlight_style() -> Style {
    Style::default().add_modifier(Modifier::BOLD)
}

pub fn block() -> Block<'static> {
    Block::bordered().border_type(BorderType::Plain)
}

pub fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect::new(area.x + (area.width - width) / 2, area.y + (area.height - height) / 2, width, height)
}

pub fn modal(frame: &mut Frame, title: &str, width: u16, body_height: u16, hint: &str) -> Option<Rect> {
    let screen = frame.area();
    if screen.width < MIN_WIDTH || screen.height < MIN_HEIGHT {
        frame.render_widget(Clear, screen);
        frame.render_widget(
            Paragraph::new("KSeF Bot\nResize terminal to at least 48 x 24.\nEsc to go back.")
                .wrap(Wrap { trim: false }).alignment(Alignment::Center),
            centered_rect(screen, 36, 4),
        );
        return None;
    }
    let width = width.min(screen.width - 4);
    let content_width = width - 6;
    let title = wrap_lines(&single_line(title), content_width);
    let hint = wrap_lines(hint, content_width);
    let title_height = title.len().min(3) as u16;
    let hint_height = hint.len() as u16;
    let area = centered_rect(screen, width, (body_height + title_height + hint_height + 6).min(screen.height - 4));
    frame.render_widget(Clear, area);
    frame.render_widget(block(), area);
    let inner = Rect::new(area.x + 3, area.y + 2, content_width, area.height - 4);
    let sections = Layout::vertical([
        Constraint::Length(title_height), Constraint::Length(1), Constraint::Min(1),
        Constraint::Length(1), Constraint::Length(hint_height),
    ]).split(inner);
    frame.render_widget(Paragraph::new(title.into_iter().take(title_height as usize).map(Line::from).collect::<Vec<_>>())
        .style(Style::default().add_modifier(Modifier::BOLD)), sections[0]);
    frame.render_widget(Paragraph::new(hint.into_iter().map(Line::from).collect::<Vec<_>>()), sections[4]);
    Some(sections[2])
}

fn single_line(text: &str) -> String {
    text.chars().map(|character| if character.is_control() { ' ' } else { character }).collect()
}

pub fn wrap_lines(text: &str, width: u16) -> Vec<String> {
    let width = usize::from(width.max(1));
    let mut lines = Vec::new();
    for source in text.lines() {
        let source = source.replace('\t', "    ");
        let mut line = String::new();
        let mut used = 0;
        for word in source.split_inclusive(char::is_whitespace) {
            let word_width = Line::from(word.trim_end()).width();
            if used > 0 && word_width <= width && used + word_width > width {
                lines.push(line.trim_end().to_string());
                line.clear();
                used = 0;
            }
            let span = Span::raw(word);
            for grapheme in span.styled_graphemes(Style::default()) {
                let size = Line::from(grapheme.symbol).width();
                if used + size > width {
                    lines.push(std::mem::take(&mut line));
                    used = 0;
                }
                line.push_str(grapheme.symbol);
                used += size;
            }
        }
        lines.push(line);
    }
    if lines.is_empty() { lines.push(String::new()); }
    lines
}

pub fn clipped_line(text: &str, width: u16) -> String {
    let text = single_line(text);
    if Line::from(text.as_str()).width() <= width as usize { return text; }
    if width <= 3 { return ".".repeat(width as usize); }
    let mut clipped = String::new();
    let span = Span::raw(text);
    for grapheme in span.styled_graphemes(Style::default()) {
        if Line::from(clipped.as_str()).width() + Line::from(grapheme.symbol).width() > width.saturating_sub(3) as usize { break; }
        clipped.push_str(grapheme.symbol);
    }
    clipped.push_str("...");
    clipped
}

pub fn navigate(selected: &mut usize, key: KeyCode, count: usize) {
    if count == 0 {
        *selected = 0;
        return;
    }
    *selected = (*selected).min(count - 1);
    match key {
        KeyCode::Tab | KeyCode::Down | KeyCode::Char('j') => *selected = (*selected + 1) % count,
        KeyCode::BackTab | KeyCode::Up | KeyCode::Char('k') => *selected = (*selected + count - 1) % count,
        KeyCode::Home => *selected = 0,
        KeyCode::End => *selected = count - 1,
        KeyCode::PageDown => *selected = (*selected + 10).min(count - 1),
        KeyCode::PageUp => *selected = selected.saturating_sub(10),
        _ => {}
    }
}

pub fn draw_select(frame: &mut Frame, title: &str, choices: &[String], state: &mut ListState) {
    let selected = state.selected().unwrap_or_default();
    let hint = format!("Tab/Shift+Tab or Up/Down: move | Enter: select\nEsc: back   {}/{}", selected + usize::from(!choices.is_empty()), choices.len());
    let Some(area) = modal(frame, title, 76, (choices.len().min(14) * 2).max(1) as u16, &hint) else { return; };
    if choices.is_empty() {
        frame.render_widget(Paragraph::new("No options available. Press Esc to go back."), area);
        return;
    }
    let spaced = area.height as usize >= choices.len() * 2;
    let items: Vec<_> = choices.iter().map(|choice| {
        let label = Line::from(clipped_line(choice, area.width.saturating_sub(2)));
        ListItem::new(if spaced { vec![label, Line::default()] } else { vec![label] })
    }).collect();
    frame.render_stateful_widget(
        List::new(items).highlight_symbol(SELECTOR),
        area, state,
    );
}

pub fn select_index(terminal: &mut Tui, title: &str, choices: &[String], selected: &mut usize) -> Result<Option<usize>> {
    *selected = (*selected).min(choices.len().saturating_sub(1));
    let mut state = ListState::default().with_selected(Some(*selected));
    loop {
        terminal.draw(|frame| draw_select(frame, title, choices, &mut state))?;
        match read_key()? {
            KeyCode::Enter if !choices.is_empty() => return Ok(Some(*selected)),
            KeyCode::Esc | KeyCode::Char('q') => return Ok(None),
            key => {
                navigate(selected, key, choices.len());
                state.select(Some(*selected));
            }
        }
    }
}

pub fn select(terminal: &mut Tui, title: &str, choices: &[String]) -> Result<Option<String>> {
    Ok(select_index(terminal, title, choices, &mut 0)?.map(|index| choices[index].clone()))
}

fn buttons(frame: &mut Frame, area: Rect, labels: &[&str], selected: Option<usize>) {
    let columns = Layout::horizontal(vec![Constraint::Ratio(1, labels.len() as u32); labels.len()]).split(area);
    for (index, label) in labels.iter().enumerate() {
        let style = if selected == Some(index) { highlight_style() } else { Style::default() };
        frame.render_widget(Paragraph::new(format!("[ {label} ]")).alignment(Alignment::Center).style(style), columns[index]);
    }
}

pub fn draw_confirm(frame: &mut Frame, title: &str, selected: usize) {
    if let Some(area) = modal(frame, title, 64, 1, "Tab/Shift+Tab: move | Enter: confirm | Esc: no") {
        buttons(frame, area, &["Yes", "No"], Some(selected));
    }
}

pub fn confirm(terminal: &mut Tui, title: &str, default: bool) -> Result<bool> {
    let mut selected = usize::from(!default);
    loop {
        terminal.draw(|frame| draw_confirm(frame, title, selected))?;
        match read_key()? {
            KeyCode::Enter => return Ok(selected == 0),
            KeyCode::Esc | KeyCode::Char('q') => return Ok(false),
            KeyCode::Left | KeyCode::Right => selected = 1 - selected,
            key => navigate(&mut selected, key, 2),
        }
    }
}

pub struct TextInput {
    value: String,
    cursor: usize,
    focus: usize,
}

impl TextInput {
    pub fn new(value: &str) -> Self {
        let value = single_line(value);
        Self { cursor: value.len(), value, focus: 0 }
    }

    fn previous(&self) -> usize {
        Span::raw(&self.value[..self.cursor]).styled_graphemes(Style::default()).last()
            .map_or(0, |grapheme| self.cursor - grapheme.symbol.len())
    }

    fn next(&self) -> usize {
        self.cursor + Span::raw(&self.value[self.cursor..]).styled_graphemes(Style::default()).next()
            .map_or(0, |grapheme| grapheme.symbol.len())
    }

    pub fn handle(&mut self, key: KeyCode) -> Option<Option<String>> {
        match key {
            KeyCode::Esc => return Some(None),
            KeyCode::Enter => return Some((self.focus != 2).then(|| self.value.clone())),
            KeyCode::Tab => self.focus = (self.focus + 1) % 3,
            KeyCode::BackTab => self.focus = (self.focus + 2) % 3,
            _ if self.focus != 0 => {}
            KeyCode::Left => self.cursor = self.previous(),
            KeyCode::Right => self.cursor = self.next(),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.value.len(),
            KeyCode::Backspace if self.cursor > 0 => {
                let previous = self.previous();
                self.value.drain(previous..self.cursor);
                self.cursor = previous;
            }
            KeyCode::Delete if self.cursor < self.value.len() => { self.value.drain(self.cursor..self.next()); }
            KeyCode::Char(character) if !character.is_control() => {
                self.value.insert(self.cursor, character);
                self.cursor += character.len_utf8();
            }
            _ => {}
        }
        None
    }

    pub fn visible(&self, width: u16) -> (&str, u16) {
        let mut start = 0;
        while start < self.cursor && Line::from(&self.value[start..self.cursor]).width() >= usize::from(width.max(1)) {
            start += Span::raw(&self.value[start..]).styled_graphemes(Style::default()).next().unwrap().symbol.len();
        }
        (&self.value[start..], Line::from(&self.value[start..self.cursor]).width() as u16)
    }
}

pub fn draw_input(frame: &mut Frame, title: &str, input: &TextInput) {
    let Some(area) = modal(frame, title, 76, 5, "Tab/Shift+Tab: focus | Enter: continue | Esc: back\nLeft/Right, Home/End: cursor | Backspace/Delete: edit") else { return; };
    let sections = Layout::vertical([Constraint::Length(3), Constraint::Length(1), Constraint::Length(1)]).split(area);
    let field = block().padding(Padding::horizontal(1))
        .border_style(if input.focus == 0 { Style::default().add_modifier(Modifier::BOLD) } else { Style::default() });
    let inner = field.inner(sections[0]);
    let (visible, cursor) = input.visible(inner.width);
    frame.render_widget(Paragraph::new(visible).block(field), sections[0]);
    buttons(frame, sections[2], &["Continue", "Cancel"], input.focus.checked_sub(1));
    if input.focus == 0 && inner.width > 0 && inner.height > 0 {
        frame.set_cursor_position((inner.x + cursor.min(inner.width - 1), inner.y));
    }
}

pub fn text_input(terminal: &mut Tui, title: &str, initial: &str) -> Result<Option<String>> {
    let mut input = TextInput::new(initial);
    loop {
        terminal.draw(|frame| draw_input(frame, title, &input))?;
        if let Some(result) = input.handle(read_key()?) {
            return Ok(result);
        }
    }
}

pub fn draw_message(frame: &mut Frame, title: &str, lines: &[String], scroll: &mut u16) -> u16 {
    let Some(area) = modal(frame, title, 88, 12, "Up/Down, PgUp/PgDn: scroll | Home/End: jump\nEnter/Esc: continue") else { return 1; };
    let lines = wrap_lines(&lines.join("\n"), area.width);
    let max_scroll = lines.len().saturating_sub(area.height as usize).min(u16::MAX as usize) as u16;
    *scroll = (*scroll).min(max_scroll);
    frame.render_widget(Paragraph::new(lines.into_iter().map(Line::from).collect::<Vec<_>>()).scroll((*scroll, 0)), area);
    area.height
}

pub fn message(terminal: &mut Tui, title: &str, lines: &[String]) -> Result<()> {
    if lines.is_empty() { return Ok(()); }
    let mut scroll = 0u16;
    loop {
        let mut page = 1;
        terminal.draw(|frame| page = draw_message(frame, title, lines, &mut scroll))?;
        match read_key()? {
            KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q') => return Ok(()),
            KeyCode::Down | KeyCode::Char('j') => scroll = scroll.saturating_add(1),
            KeyCode::Up | KeyCode::Char('k') => scroll = scroll.saturating_sub(1),
            KeyCode::PageDown => scroll = scroll.saturating_add(page),
            KeyCode::PageUp => scroll = scroll.saturating_sub(page),
            KeyCode::Home => scroll = 0,
            KeyCode::End => scroll = u16::MAX,
            _ => {}
        }
    }
}

pub fn required_field(terminal: &mut Tui, title: &str) -> Result<Option<String>> {
    validated_input(terminal, title, "", |value| {
        if value.is_empty() { Some("This field is required.".to_string()) } else { None }
    })
}

pub fn optional_field(terminal: &mut Tui, title: &str) -> Result<Option<String>> {
    Ok(text_input(terminal, title, "")?.map(|value| value.trim().to_string()).filter(|value| !value.is_empty()))
}

fn validated_input(terminal: &mut Tui, title: &str, initial: &str, validate: impl Fn(&str) -> Option<String>) -> Result<Option<String>> {
    let mut value = initial.to_string();
    loop {
        let Some(edited) = text_input(terminal, title, &value)? else { return Ok(None); };
        value = edited.trim().to_string();
        match validate(&value) {
            None => return Ok(Some(value)),
            Some(error) => message(terminal, "Check this field", &[error])?,
        }
    }
}

fn validate_digits(value: &str, length: usize, optional: bool) -> Option<String> {
    if (optional && value.is_empty()) || (value.len() == length && value.chars().all(|character| character.is_ascii_digit())) {
        None
    } else {
        Some(format!("Enter exactly {length} digits{}.", if optional { ", or leave blank" } else { "" }))
    }
}

pub fn digits_field(terminal: &mut Tui, title: &str, length: usize) -> Result<Option<String>> {
    validated_input(terminal, title, "", |value| validate_digits(value, length, false))
}

pub fn optional_digits_field(terminal: &mut Tui, title: &str, length: usize) -> Result<Option<String>> {
    Ok(validated_input(terminal, title, "", |value| validate_digits(value, length, true))?.filter(|value| !value.is_empty()))
}

pub fn edit_required(terminal: &mut Tui, title: &str, current: &str) -> Result<String> {
    Ok(validated_input(terminal, title, current, |value| {
        if value.is_empty() { Some("This field is required.".to_string()) } else { None }
    })?.unwrap_or_else(|| current.to_string()))
}

pub fn edit_optional(terminal: &mut Tui, title: &str, current: Option<&str>) -> Result<Option<String>> {
    Ok(match text_input(terminal, title, current.unwrap_or(""))? {
        None => current.map(str::to_string),
        Some(value) => Some(value.trim().to_string()).filter(|value| !value.is_empty()),
    })
}

pub fn edit_digits(terminal: &mut Tui, title: &str, current: Option<&str>, length: usize) -> Result<String> {
    Ok(validated_input(terminal, title, current.unwrap_or(""), |value| validate_digits(value, length, false))?
        .unwrap_or_else(|| current.unwrap_or_default().to_string()))
}

pub fn edit_optional_digits(terminal: &mut Tui, title: &str, current: Option<&str>, length: usize) -> Result<Option<String>> {
    Ok(match validated_input(terminal, title, current.unwrap_or(""), |value| validate_digits(value, length, true))? {
        None => current.map(str::to_string),
        Some(value) => Some(value).filter(|value| !value.is_empty()),
    })
}
