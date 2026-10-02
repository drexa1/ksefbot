use super::{AppUser, Language, SettlementType};
use crate::tui::prompt::Prompter;
use crate::{cf_client_id, cf_client_secret, cf_worker_url};
use anyhow::Result;

pub async fn edit_profile(app_user: &AppUser, prompter: &mut impl Prompter) -> Result<()> {
    let mut user = app_user.clone();
    let original = user.clone();
    loop {
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
            .map(|(key, value)| format!("{key}: {value}"))
            .collect();
        if user != original {
            fields.push("âœ… Save changes".to_string());
        }
        fields.push("âŒ Cancel".to_string());
        let field = prompter.select("Edit user settings", &fields)?;
        let selected_index = fields.iter().position(|choice| choice == &field).unwrap();
        match keys.get(selected_index).copied().unwrap_or_default() {
            "Email" => user.email = edit_required(prompter, "Email:", &user.email)?,
            "Phone" => user.phone = Some(edit_required(prompter, "Phone:", user.phone.as_deref().unwrap_or(""))?),
            "Language" => user.language = Some(edit_language(prompter)?),
            "Default item name" => user.default_item_name = edit_optional(prompter, "Default item name:", user.default_item_name.as_deref())?,
            "Default hourly rate" => user.default_hourly_rate = edit_optional_rate(prompter, user.default_hourly_rate)?,
            "Settlement type" => user.settlement_type = Some(edit_settlement_type(prompter)?),
            "Bank name" => user.bank_name = edit_optional(prompter, "Bank name:", user.bank_name.as_deref())?,
            "Bank account number" => user.bank_account_number = edit_optional_digits(prompter, "Bank account number:", user.bank_account_number.as_deref(), 26)?,
            _ if field.starts_with("âœ…") => break,
            _ => return Ok(()),
        }
    }
    if !prompter.confirm("Save changes to your profile?", true)? {
        prompter.pause()?;
        return Ok(());
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
    prompter.info("âœ… Profile updated successfully.")?;
    prompter.pause()
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

fn edit_language(prompter: &mut impl Prompter) -> Result<Language> {
    let choice = prompter.select("Language:", &["English".to_string(), "Polish".to_string()])?;
    Ok(if choice == "Polish" { Language::Pl } else { Language::En })
}

fn edit_settlement_type(prompter: &mut impl Prompter) -> Result<SettlementType> {
    let choice = prompter.select("Settlement type:", &["Monthly".to_string(), "Quarterly".to_string()])?;
    Ok(if choice == "Quarterly" { SettlementType::Quarterly } else { SettlementType::Monthly })
}

fn edit_required(prompter: &mut impl Prompter, message: &str, current: &str) -> Result<String> {
    loop {
        let value = prompter.text(message, current)?;
        if !value.trim().is_empty() {
            return Ok(value.trim().to_string());
        }
        prompter.info("This field is required.")?;
    }
}

fn edit_optional(prompter: &mut impl Prompter, message: &str, current: Option<&str>) -> Result<Option<String>> {
    let value = prompter.text(message, current.unwrap_or(""))?;
    Ok(if value.trim().is_empty() { None } else { Some(value.trim().to_string()) })
}

fn edit_optional_rate(prompter: &mut impl Prompter, current: Option<f64>) -> Result<Option<f64>> {
    loop {
        let current_text = current.map(|rate| rate.to_string()).unwrap_or_default();
        let value = prompter.text("Default hourly rate:", &current_text)?;
        if value.trim().is_empty() {
            return Ok(None);
        }
        match value.trim().parse::<f64>() {
            Ok(rate) => return Ok(Some(rate)),
            Err(_) => prompter.info("Please enter a valid number, or leave blank.")?,
        }
    }
}

fn edit_optional_digits(prompter: &mut impl Prompter, message: &str, current: Option<&str>, length: usize) -> Result<Option<String>> {
    loop {
        let value = prompter.text(message, current.unwrap_or(""))?;
        if value.trim().is_empty() {
            return Ok(None);
        }
        if value.len() == length && value.chars().all(|character| character.is_ascii_digit()) {
            return Ok(Some(value));
        }
        prompter.info(&format!("Please enter exactly {length} digits, or leave blank."))?;
    }
}
