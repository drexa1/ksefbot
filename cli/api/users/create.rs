use crate::{api_key, cf_client_id, cf_client_secret, cf_worker_url};

pub async fn create_app_user(user: &serde_json::Value) -> anyhow::Result<()> {
    let response = crate::api::client::http_client()
        .post(format!("{}/app/users", cf_worker_url!()))
        .header("CF-Access-Client-Id", cf_client_id!())
        .header("CF-Access-Client-Secret", cf_client_secret!())
        .header("X-API-Key", api_key!())
        .json(user)
        .send()
        .await?;
    let status = response.status();
    let body: serde_json::Value = response.json().await?;
    if !status.is_success() {
        anyhow::bail!("{}", body["error"].as_str().unwrap_or("Could not create user"));
    }
    Ok(())
}