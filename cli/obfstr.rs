// -------------------------------------------------------------------------------------------------
// Cloudflare auth
// -------------------------------------------------------------------------------------------------

#[macro_export]
macro_rules! cf_worker_url {
    () => {
        obfstr::obfstr!("https://ksefbot-api.druizbarbero.workers.dev")
    };
}

#[macro_export]
macro_rules! cf_client_id {
    () => {
        obfstr::obfstr!("ec663ac850bcfb511f8439df52635f55.access")
    };
}

#[macro_export]
macro_rules! cf_client_secret {
    () => {
        obfstr::obfstr!("c7a9fa6e563c9e290cb941b39de135a48878653d9e64476e3970fbd8466816a1")
    };
}

// -------------------------------------------------------------------------------------------------
// API Protected routes
// -------------------------------------------------------------------------------------------------

#[macro_export]
macro_rules! api_key {
    () => {
        obfstr::obfstr!("55oUrQjUlwlZYCS30WGfpMMZCiQkfKpt")
    };
}

// -------------------------------------------------------------------------------------------------
// SSO Login
// -------------------------------------------------------------------------------------------------

#[macro_export]
macro_rules! microsoft_client_id {
    () => {
        obfstr::obfstr!("162f74b1-fa6c-4c12-a0dd-9c1d77ce7b11")
    };
}

#[macro_export]
macro_rules! google_client_id {
    () => {
        obfstr::obfstr!("19016612694-nr24bk45o7u24mvm2227ic4oorhhvmdt.apps.googleusercontent.com")
    };
}

#[macro_export]
macro_rules! google_client_secret {
    () => {
        obfstr::obfstr!("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")
    };
}