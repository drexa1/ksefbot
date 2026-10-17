use crate::api::customers::create::{ContractorLookup, NewContractor, lookup_customer, preview_customer, save_customer};
use crate::api::users::update::{edit_optional_hourly_rate, edit_settlement_type};
use crate::api::users::{Language, create::create_app_user};
use crate::login::AuthUser;
use anyhow::Context;
use crossterm::{cursor::MoveToColumn, execute, style::{Color, Stylize}, terminal::{Clear, ClearType}};
use inquire::{Confirm, Select, Text};
use std::{io::{Write, stdout}, time::Duration};

pub const BANK_NAMES: [&str; 7] = [
    "Alior Bank",
    "Bank Millennium",
    "BNP Paribas",
    "ING Bank Śląski",
    "mBank",
    "PKO Bank Polski",
    "Santander Bank Polska"
];

pub async fn onboard(user: &AuthUser) -> anyhow::Result<()> {
    println!();
    println!("It looks like we haven't worked together just yet. We just need a few details to get you started,");
    println!(" ■ Email/phone, for logging in");
    println!(" ■ Your company tax ID");
    println!(" ■ A token to interact with KSeF");
    println!(" ■ (Optionally) banking details, for taxes automation");
    println!("It'll take just a couple minutes, and you'll be set.");
    println!();
    println!("{}", "Let's start with your details:".bold());
    let contractor = prompt_company().await?;
    let nip = contractor.nip.clone();
    let email = prompt_required("Email:", user.email.as_deref().unwrap_or(""), "name@example.com", valid_email)?;
    let phone = prompt_required("Phone number (+country code, no spaces):", user.phone.as_deref().unwrap_or(""), "+48111222333", |value| {
        value.starts_with("+48") && value.len() == 12 && value[3..].bytes().all(|byte| byte.is_ascii_digit())
    })?;
    let language = Select::new("Preferred language:", vec!["English", "Polish"]).prompt()?;
    let language = if language == "Polish" { Language::Pl } else { Language::En };
    println!();
    println!("Your company's main product/service description will be used when creating new invoices, saving you time.");
    let default_item_name = prompt_required(
        "Description of the company main service/product:",
        "",
        "Enter a description",
        |value| !value.trim().is_empty()
    )?;
    let default_hourly_rate = edit_optional_hourly_rate(None)?;
    let settlement_type = edit_settlement_type(None)?;
    println!();
    println!("{}", "  How to get a KSeF token if you don't have one already".bold().underlined());
    token_instructions();
    safety_notice();
    let ksef_api_token = prompt_required("[Required] KSeF token:", "", "Paste the full token", |value| valid_token(value, &nip))?;
    println!();
    println!("{}", "Banking".bold());
    let mut banks = BANK_NAMES.to_vec();
    banks.push("⏩Skip");
    let bank = Select::new("[Optional] Bank branch:", banks).with_page_size(8).prompt()?;
    let bank_name = if bank == "Skip" { None } else { Some(bank) };
    let bank_account_number = prompt_required(
        "Bank account number (26 digits):",
        "",
        "Enter exactly 26 digits",
        |value| value.len() == 26 && value.bytes().all(|b| b.is_ascii_digit())
    )?;
    create_app_user(&serde_json::json!({
        "id": nip,
        "email": email.trim(),
        "phone": phone.trim(),
        "language": language,
        "defaultItemName": default_item_name,
        "defaultHourlyRate": default_hourly_rate,
        "settlementType": settlement_type,
        "ksefApiToken": ksef_api_token.trim(),
        "bankName": bank_name,
        "bankAccountNumber": bank_account_number
    })).await.context("Error configuring first time user")?;
    save_customer(&nip, &contractor).await.context("Failed to save user contractor data")?;
    retrieve_invoices(&nip).await.context("  Retrieving recent invoices failed")?;
    println!("  🟢 Your user is configured, you should be good to login");
    Ok(())
}

async fn prompt_company() -> anyhow::Result<NewContractor> {
    for _ in 1..=3 {
        let nip = Text::new("Company tax ID (NIP):").with_placeholder("1234567890").prompt()?.trim().to_string();
        if nip.len() != 10 || !nip.bytes().all(|byte| byte.is_ascii_digit()) {
            println!("NIP should have exactly 10 digits.");
            continue;
        }
        match lookup_customer::<ContractorLookup>(&nip, "gov/contractors", &[("nip", &nip), ("profile", "user")]).await {
            Ok(Some(mut found)) if found.active => {
                let contractor = &mut found.contractor;
                preview_customer(&contractor.name, &contractor.nip, contractor.regon.as_deref(), &contractor.address_l1, &contractor.country_code);
                println!();
                if Confirm::new("Is this your company?:").with_default(true).prompt()? {
                    contractor.notes = Some("seller".to_string());
                    return Ok(found.contractor);
                }
            }
            Ok(_) => println!("We couldn't find a company for NIP '{nip}'"),
            Err(error) => {
                println!("Company lookup failed: {error}");
                println!();
            }
        }
    }
    anyhow::bail!("Company lookup failed.");
}

fn prompt_required(message: &str, initial: &str, placeholder: &str, valid: impl Fn(&str) -> bool) -> anyhow::Result<String> {
    for _ in 1..=3 {
        let value = Text::new(message).with_initial_value(initial).with_placeholder(placeholder).prompt()?;
        let value = value.trim();
        if valid(value) {
            return Ok(value.to_string());
        }
        println!("Please enter a valid value. {placeholder}.");
    }
    anyhow::bail!("Onboarding failed after 3 invalid attempts for {message}.");
}

fn valid_email(value: &str) -> bool {
    !value.chars().any(char::is_whitespace) && value.split_once('@').is_some_and(|(name, domain)| {
        !name.is_empty() &&
        !domain.contains('@') &&
        domain.split_once('.').is_some_and(|(host, suffix)| !host.is_empty() && !suffix.is_empty())
    })
}

fn valid_token(value: &str, nip: &str) -> bool {
    let parts: Vec<_> = value.split('|').collect();
    if parts.len() != 3 || parts[1] != format!("nip-{nip}") || parts[2].len() != 64 || !parts[2].bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return false;
    }
    let reference: Vec<_> = parts[0].split('-').collect();
    reference.len() == 5 && reference[0].len() == 8 && reference[0].bytes().all(|byte| byte.is_ascii_digit())
        && chrono::NaiveDate::parse_from_str(reference[0], "%Y%m%d").is_ok() && reference[1] == "EC"
        && reference[2..].iter().zip([10, 10, 2]).all(|(part, length)| part.len() == length && part.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

fn token_instructions() {
    let url = "https://ap.ksef.mf.gov.pl";
    let link = format!("\x1b]8;;{url}\x1b\\{url}\x1b]8;;\x1b\\");
    let lines = [
        format!("1. Open {}", link.blue().bold()),
        "   and sign in with your company's NIP and preferred identity verification method.".to_string(),
        format!("2. Go to {} > {}; name it 'KSeF Bot' and allow:", "Tokeny".blue().bold(), "Generuj token".blue().bold()),
        format!("   ✅ {}", "wystawianie faktur / issuing invoices"),
        format!("   ✅ {}", "przeglądanie faktur / viewing invoices"),
        format!("   ✅ {}", "przeglądanie historii sesji (generowanie UPO) / viewing session history (generating UPO)"),
        format!("3. Click {}, then {} ↻.", "Generuj token".blue().bold(), "Odśwież/Refresh".blue().bold()),
        "  📋 Copy the token shown once and paste it below.".to_string()
    ];
    for line in lines {
        println!("{line}");
    }
}

fn safety_notice() {
    println!();
    println!(" 🔐 {}", "Is this safe?".bold());
    let lines = [
        "A KSeF token lets applications interact with KSeF on your behalf.",
        "You choose its permissions and you can revoke it at any time to prevent its usage.",
        "Cloudflare automatically encrypts data at rest, and protects data in transit with TLS."
    ];
    let width = lines.iter().map(|line| line.len()).max().unwrap_or(0);
    println!("┌{}┐", "─".repeat(width + 2));
    for line in lines {
        println!("│ {:<width$} │", line);
    }
    println!("└{}┘", "─".repeat(width + 2));
}

fn backfill_request(user_id: &str, method: reqwest::Method) -> reqwest::RequestBuilder {
    crate::api::client::http_client().request(method, format!("{}/ksef/backfill", crate::cf_worker_url!()))
        .header("CF-Access-Client-Id", crate::cf_client_id!())
        .header("CF-Access-Client-Secret", crate::cf_client_secret!())
        .header("X-API-Key", crate::api_key!())
        .header("X-User-Id", user_id)
        .timeout(Duration::from_secs(30))
}

async fn retrieve_invoices(user_id: &str) -> anyhow::Result<()> {
    println!("  Retrieving recent invoices...");
    let response = backfill_request(user_id, reqwest::Method::POST).send().await?;
    let status = response.status();
    let body: serde_json::Value = response.json().await?;
    if !status.is_success() && !(status == reqwest::StatusCode::CONFLICT && body["existing"] == true) {
        anyhow::bail!("{}", body["error"].as_str().unwrap_or("Could not start invoice retrieval"));
    }
    let mut output = stdout();
    let mut animation = tokio::time::interval(Duration::from_millis(80));
    let mut progress = 0.0_f64;
    let mut target = 0.0_f64;
    let mut downloaded = 0;
    let result = async {
        loop {
            let poll = async {
                tokio::time::sleep(Duration::from_secs(2)).await;
                let response = backfill_request(user_id, reqwest::Method::GET).send().await?;
                let status = response.status();
                let body: serde_json::Value = response.json().await?;
                if !status.is_success() {
                    anyhow::bail!("{}", body["error"].as_str().unwrap_or("Could not check invoice retrieval"));
                }
                Ok::<_, anyhow::Error>(body)
            };
            tokio::pin!(poll);
            let body = loop {
                tokio::select! {
                    result = &mut poll => break result?,
                    _ = animation.tick() => {
                        progress = (progress + 0.01).min(target);
                        draw_progress(&mut output, progress)?;
                    }
                }
            };
            downloaded = body["invoicesDownloaded"].as_u64().context("Missing invoice retrieval count")?;
            target = target.max(body["progress"].as_f64().context("Missing invoice retrieval progress")?).min(0.99);
            match body["status"].as_str() {
                Some("completed") => {
                    while progress < 1.0 {
                        animation.tick().await;
                        progress = (progress + 0.01).min(1.0);
                        draw_progress(&mut output, progress)?;
                    }
                    return Ok(false);
                },
                Some("throttled") => return Ok(true),
                Some("failed") => anyhow::bail!("{}", body["error"].as_str().unwrap_or("Invoice retrieval failed")),
                Some("queued" | "running") => {},
                _ => anyhow::bail!("Unknown invoice retrieval status"),
            }
        }
    }.await;
    if matches!(&result, Ok(false)) {
        println!();
    } else {
        execute!(output, MoveToColumn(0), Clear(ClearType::CurrentLine))?;
    }
    if result? {
        println!("  {downloaded} invoices retrieved so far. KSeF rate limit reached, retrieval will resume in background.");
    } else {
        println!("  {downloaded} invoices retrieved.");
    }
    Ok(())
}

fn draw_progress(output: &mut impl Write, progress: f64) -> anyhow::Result<()> {
    let units = (progress * 24.0 * 8.0) as usize;
    let partial = ["", "▏", "▎", "▍", "▌", "▋", "▊", "▉"][units % 8];
    let filled = format!("{}{partial}", "█".repeat(units / 8)).with(Color::Rgb { r: 160, g: 80, b: 220 });
    let empty = " ".repeat(24 - units / 8 - usize::from(!partial.is_empty()));
    write!(
        output,
        "\r  Retrieving your recent invoices [{filled}{empty}] {:3.0}%",
        (progress * 100.0).floor()
    )?;
    output.flush()?;
    Ok(())
}