use crate::{api_key};

async fn get_user_by_email(email: &str) -> anyhow::Result<Option<serde_json::Value>> {
    let json: serde_json::Value = reqwest::Client::new()
        .get(format!("{}/app/users", cf_worker_url!()))
        .query(&[("email", email)])
        .query(&[("onboarding", true)])
        .header("X-API-Key", api_key!())
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .send()
        .await?
        .json()
        .await?;
    if json["success"].as_bool() != Some(true) {
        return Ok(None);
    }
    let users = json["result"].as_array().cloned().unwrap_or_default();
    Ok(users.into_iter().next())
}