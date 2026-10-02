use anyhow::Result;

/// Shared UI surface for TUI flows.
pub trait Prompter {
    fn info(&mut self, message: &str) -> Result<()>;
    fn select(&mut self, title: &str, choices: &[String]) -> Result<String>;
    fn text(&mut self, title: &str, initial: &str) -> Result<String>;
    fn confirm(&mut self, title: &str, default: bool) -> Result<bool>;
    fn pause(&mut self) -> Result<()>;
}
