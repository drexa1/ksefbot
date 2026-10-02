use super::{AppUser, Language, SettlementType};
use crate::{cf_client_id, cf_client_secret, cf_worker_url};
use anyhow::Result;
use crossterm::style::Stylize;
use inquire::{Confirm, Select, Text};

pub async fn edit_profile(app_user: &AppUser) -> Result<bool> {
    let mut user = app_user.clone();
    let original = user.clone();
    loop {
        println!();
        let keys = ["Email", "Phone", "Language", "Default hourly rate", "Default item name", "Settlement type", "Bank name", "Bank account number"];
        let values = [
            user.email.clone(),
            user.phone.clone().unwrap_or_default(),
            language_label(&user.language).to_string(),
            user.default_hourly_rate.map(|rate| rate.to_string()).unwrap_or_default(),
            user.default_item_name.clone().unwrap_or_default(),
            settlement_type_label(&user.settlement_type).to_string(),
            user.bank_name.clone().unwrap_or_default(),
            user.bank_account_number.clone().unwrap_or_default(),
        ];
        let mut fields: Vec<String> = keys.iter().zip(values.iter())
            .map(|(key, value)| format!("{}: {value}", key.bold()))
            .collect();
        if user != original {
            fields.push("✅ Save changes".to_string());
        }
        fields.push("❌ Cancel".to_string());
        let field = Select::new("Edit user settings", fields.clone()).with_page_size(fields.len()).prompt()?;
        let selected_index = fields.iter().position(|choice| choice == &field).unwrap();
        match keys.get(selected_index).copied().unwrap_or_default() {
            "Email" => user.email = edit_required("Email:", &user.email)?,
            "Phone" => user.phone = Some(edit_required("Phone:", user.phone.as_deref().unwrap_or(""))?),
            "Language" => user.language = Some(edit_language(user.language.as_ref())?),
            "Default item name" => user.default_item_name = edit_optional("Default item name:", user.default_item_name.as_deref())?,
            "Default hourly rate" => user.default_hourly_rate = edit_optional_rate(user.default_hourly_rate)?,
            "Settlement type" => user.settlement_type = Some(edit_settlement_type(user.settlement_type.as_ref())?),
            "Bank name" => user.bank_name = edit_optional("Bank name:", user.bank_name.as_deref())?,
            "Bank account number" => user.bank_account_number = edit_optional_digits("Bank account number:", user.bank_account_number.as_deref(), 26)?,
            _ if field.starts_with("✅") => break,
            _ => return Ok(false),
        }
    }
    println!();
    if !Confirm::new("Save changes to your profile?").with_default(true).prompt()? {
        return Ok(true);
    }
    let response = crate::api::client::http_client()
        .put(format!("{}/app/users", cf_worker_url!()))
        .header("CF-Access-Client-Id", cf_client_id!())
        .header("CF-Access-Client-Secret", cf_client_secret!())
        .header("X-API-Key", app_user.api_key.as_deref().ok_or_else(|| anyhow::anyhow!("User has no API key configured"))?)
        .header("X-User-Id", &app_user.id)
        .header("Accept", "application/json")
        .json(&user)
        .send()
        .await?;
    let status = response.status();
    let body: serde_json::Value = response.json().await?;
    if !status.is_success() || body["success"].as_bool() != Some(true) {
        anyhow::bail!("Profile update failed: {}", body["error"].as_str().unwrap_or("unknown error"));
    }
    println!();
    println!("  ✅ Profile updated successfully.");
    Ok(true)
}

fn language_label(language: &Option<Language>) -> &'static str {
    match language {
        Some(Language::En) => "English",
        Some(Language::Pl) => "Polish",
        None => ""
    }
}

fn settlement_type_label(settlement_type: &Option<SettlementType>) -> &'static str {
    match settlement_type {
        Some(SettlementType::Monthly) => "Monthly",
        Some(SettlementType::Quarterly) => "Quarterly",
        None => ""
    }
}

fn edit_language(current: Option<&Language>) -> Result<Language> {
    let choices = vec!["English", "Polish"];
    let starting_cursor = match current {
        Some(Language::Pl) => 1,
        _ => 0,
    };
    let choice = Select::new("Language:", choices).with_starting_cursor(starting_cursor).prompt()?;
    Ok(if choice == "Polish" { Language::Pl } else { Language::En })
}

fn edit_settlement_type(current: Option<&SettlementType>) -> Result<SettlementType> {
    let choices = vec!["Monthly", "Quarterly"];
    let starting_cursor = match current {
        Some(SettlementType::Quarterly) => 1,
        _ => 0,
    };
    let choice = Select::new("Settlement type:", choices).with_starting_cursor(starting_cursor).prompt()?;
    Ok(if choice == "Quarterly" { SettlementType::Quarterly } else { SettlementType::Monthly })
}

fn edit_required(message: &str, current: &str) -> Result<String> {
    loop {
        let value = Text::new(message).with_initial_value(current).prompt()?;
        if !value.trim().is_empty() {
            return Ok(value.trim().to_string());
        }
        println!("  {}", "This field is required.".dark_yellow());
    }
}

fn edit_optional(message: &str, current: Option<&str>) -> Result<Option<String>> {
    let mut prompt = Text::new(message);
    if let Some(current) = current {
        prompt = prompt.with_initial_value(current);
    }
    let value = prompt.prompt()?;
    Ok(if value.trim().is_empty() { None } else { Some(value.trim().to_string()) })
}

fn edit_optional_rate(current: Option<f64>) -> Result<Option<f64>> {
    loop {
        let mut prompt = Text::new("Default hourly rate:");
        let current_text = current.map(|rate| rate.to_string());
        if let Some(current_text) = current_text.as_deref() {
            prompt = prompt.with_initial_value(current_text);
        }
        let value = prompt.prompt()?;
        if value.trim().is_empty() {
            return Ok(None);
        }
        match value.trim().parse::<f64>() {
            Ok(rate) => return Ok(Some(rate)),
            Err(_) => println!("  {}", "Please enter a valid number, or leave blank.".dark_yellow()),
        }
    }
}

fn edit_optional_digits(message: &str, current: Option<&str>, length: usize) -> Result<Option<String>> {
    loop {
        let mut prompt = Text::new(message);
        if let Some(current) = current {
            prompt = prompt.with_initial_value(current);
        }
        let value = prompt.prompt()?;
        if value.trim().is_empty() {
            return Ok(None);
        }
        if value.len() == length && value.chars().all(|character| character.is_ascii_digit()) {
            return Ok(Some(value));
        }
        println!("  {}", format!("Please enter exactly {length} digits, or leave blank.").dark_yellow());
    }
}
