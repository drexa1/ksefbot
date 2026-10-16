use super::{AppUser, Language, SettlementType};
use crate::{cf_client_id, cf_client_secret, cf_worker_url};
use crate::onboarding::BANK_NAMES;
use anyhow::Result;
use crossterm::style::Stylize;
use inquire::{Confirm, Select, Text};

pub async fn edit_profile(app_user: &mut AppUser) -> Result<bool> {
    let mut edit_user = app_user.clone();
    let original = edit_user.clone();
    loop {
        let keys = ["Email", "Phone", "Language", "Default hourly rate", "Default invoice item description", "Settlement type", "Bank name", "Bank account number"];
        let values = [
            edit_user.email.clone().unwrap(),
            edit_user.phone.clone().unwrap(),
            language_label(&edit_user.language).to_string(),
            edit_user.default_hourly_rate.map(|rate| rate.to_string()).unwrap_or_default(),
            edit_user.default_item_name.clone().unwrap_or_default(),
            settlement_type_label(&edit_user.settlement_type).to_string(),
            edit_user.bank_name.clone().unwrap_or_default(),
            edit_user.bank_account_number.clone().unwrap_or_default(),
        ];
        let mut fields: Vec<String> = keys.iter().zip(values.iter())
            .map(|(key, value)| format!("{}: {value}", key.bold()))
            .collect();
        if edit_user != original {
            fields.push("✅ Save changes".to_string());
        }
        fields.push("❌ Cancel".to_string());
        let field = Select::new("Edit user settings", fields.clone()).with_page_size(fields.len()).prompt()?;
        let selected_index = fields.iter().position(|choice| choice == &field).unwrap();
        match keys.get(selected_index).copied().unwrap_or_default() {
            "Email" => edit_user.email = Some(edit_required("Email:", edit_user.email.as_deref().unwrap_or(""))?),
            "Phone" => edit_user.phone = Some(edit_required("Phone:", edit_user.phone.as_deref().unwrap_or(""))?),
            "Language" => edit_user.language = Some(edit_language(edit_user.language.as_ref())?),
            "Default invoice item description" => edit_user.default_item_name = edit_optional("Default invoice item description:", edit_user.default_item_name.as_deref())?,
            "Default hourly rate" => edit_user.default_hourly_rate = edit_optional_hourly_rate(edit_user.default_hourly_rate)?,
            "Settlement type" => edit_user.settlement_type = Some(edit_settlement_type(edit_user.settlement_type.as_ref())?),
            "Bank name" => edit_user.bank_name = Some(edit_bank_name(edit_user.bank_name.as_deref())?),
            "Bank account number" => edit_user.bank_account_number = edit_optional_digits("Bank account number:", edit_user.bank_account_number.as_deref(), 26)?,
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
        .header("X-API-Key", crate::api_key!())
        .header("X-User-Id", &app_user.id)
        .header("Accept", "application/json")
        .json(&edit_user)
        .send()
        .await?;
    let status = response.status();
    let body: serde_json::Value = response.json().await?;
    if !status.is_success() || body["success"].as_bool() != Some(true) {
        anyhow::bail!("Profile update failed: {}", body["error"].as_str().unwrap_or("unknown error"));
    }
    *app_user = edit_user;
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

pub fn edit_settlement_type(current: Option<&SettlementType>) -> Result<SettlementType> {
    let choices = vec!["Monthly", "Quarterly"];
    let starting_cursor = match current {
        Some(SettlementType::Quarterly) => 1,
        _ => 0,
    };
    let choice = Select::new("Settlement type:", choices).with_starting_cursor(starting_cursor).prompt()?;
    Ok(if choice == "Quarterly" { SettlementType::Quarterly } else { SettlementType::Monthly })
}

fn edit_bank_name(current: Option<&str>) -> Result<String> {
    let choices = BANK_NAMES.to_vec();
    let starting_cursor = choices.iter().position(|bank| Some(*bank) == current).unwrap_or(0);
    Ok(Select::new("Bank name:", choices).with_starting_cursor(starting_cursor).with_page_size(7).prompt()?.to_string())
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

pub fn edit_optional(message: &str, current: Option<&str>) -> Result<Option<String>> {
    let mut prompt = Text::new(message);
    if let Some(current) = current {
        prompt = prompt.with_initial_value(current);
    }
    let value = prompt.prompt()?;
    Ok(if value.trim().is_empty() { None } else { Some(value.trim().to_string()) })
}

pub fn edit_optional_hourly_rate(current: Option<f64>) -> Result<Option<f64>> {
    loop {
        let mut prompt = Text::new("[Optional] Default hourly rate (PLN/h):");
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

pub fn edit_optional_digits(message: &str, current: Option<&str>, length: usize) -> Result<Option<String>> {
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