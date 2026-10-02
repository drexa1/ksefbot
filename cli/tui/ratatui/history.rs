use super::invoices::{InvoiceType, list_invoices};
use crate::api::users::AppUser;
use crate::cf_worker_url;
use anyhow::{Context, Result, ensure};
use chrono::{Datelike, Duration, Months, NaiveDate};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::future::Future;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug)]
pub struct InvoiceHistory {
    pub months: Vec<InvoiceMonth>,
    pub start: NaiveDate,
    pub end: NaiveDate,
}

#[derive(Debug)]
pub struct InvoiceMonth {
    pub month: NaiveDate,
    pub sales: Vec<Value>,
    pub purchases: Vec<Value>,
}

#[derive(Debug, PartialEq)]
pub struct MonthlyTotal {
    pub currency: String,
    pub sales: f64,
    pub purchases: f64,
}

impl InvoiceMonth {
    pub fn invoices(&self, kind: InvoiceType) -> &[Value] {
        match kind {
            InvoiceType::Sales => &self.sales,
            InvoiceType::Purchases => &self.purchases,
        }
    }

    pub fn totals(&self) -> Result<Vec<MonthlyTotal>> {
        let mut totals = BTreeMap::<String, MonthlyTotal>::new();
        for kind in [InvoiceType::Sales, InvoiceType::Purchases] {
            for invoice in self.invoices(kind) {
                let body = &invoice["InvoiceBody"];
                let number = body["InvoiceNumber"].as_str().unwrap_or("<unknown>");
                let currency = body["CurrencyCode"].as_str()
                    .filter(|currency| !currency.trim().is_empty())
                    .with_context(|| format!("Invoice {number}: missing or invalid CurrencyCode"))?;
                let amount = body["TotalGrossAmount"].as_f64()
                    .filter(|amount| amount.is_finite())
                    .with_context(|| format!("Invoice {number}: missing, invalid or nonfinite TotalGrossAmount"))?;
                let total = totals.entry(currency.to_string()).or_insert_with(|| MonthlyTotal {
                    currency: currency.to_string(), sales: 0.0, purchases: 0.0,
                });
                let sum = match kind {
                    InvoiceType::Sales => &mut total.sales,
                    InvoiceType::Purchases => &mut total.purchases,
                };
                *sum += amount;
                ensure!(sum.is_finite(), "Nonfinite {kind} total for {currency} in {}", self.month);
            }
        }
        Ok(totals.into_values().collect())
    }
}

pub fn initial_range(today: NaiveDate) -> (NaiveDate, NaiveDate) {
    (today.with_day(1).unwrap().checked_sub_months(Months::new(2)).unwrap(), today)
}

pub fn older_range(before: NaiveDate) -> Option<(NaiveDate, NaiveDate)> {
    let start = before.with_day(1)?.checked_sub_months(Months::new(3))?;
    let end = before.pred_opt()?;
    validate_range(start, end).ok()?;
    Some((start, end))
}

fn validate_range(start: NaiveDate, end: NaiveDate) -> Result<()> {
    ensure!((1..=9999).contains(&start.year()) && (1..=9999).contains(&end.year()),
        "Invoice history dates must have years between 1 and 9999");
    ensure!(start <= end, "Invoice history start date must not be after end date");
    Ok(())
}

pub fn query_windows(start: NaiveDate, end: NaiveDate) -> Result<Vec<(NaiveDate, NaiveDate)>> {
    validate_range(start, end)?;
    let mut windows = Vec::new();
    let mut from = start;
    loop {
        let next = from.with_day(1).unwrap().checked_add_months(Months::new(3))
            .context("Invoice query window exceeds supported dates")?;
        let limit = from.checked_add_signed(Duration::days(90)).context("Invoice query window exceeds supported dates")?;
        let to = end.min(next.pred_opt().context("Invalid invoice query window end")?).min(limit);
        windows.push((from, to));
        if to == end { break; }
        from = to.succ_opt().context("Invalid invoice query window start")?;
    }
    Ok(windows)
}

fn issue_date(invoice: &Value) -> Result<NaiveDate> {
    let number = invoice["InvoiceBody"]["InvoiceNumber"].as_str().unwrap_or("<unknown>");
    let value = invoice["InvoiceBody"]["IssueDate"].as_str()
        .with_context(|| format!("Invoice {number}: missing or invalid IssueDate"))?;
    let prefix = value.get(..10)
        .filter(|_| value.len() == 10 || value.as_bytes().get(10) == Some(&b'T'))
        .with_context(|| format!("Invoice {number}: invalid IssueDate {value:?}"))?;
    let date = NaiveDate::parse_from_str(prefix, "%Y-%m-%d")
        .with_context(|| format!("Invoice {number}: invalid IssueDate {value:?}"))?;
    ensure!((1..=9999).contains(&date.year()) && date.format("%Y-%m-%d").to_string() == prefix,
        "Invoice {number}: invalid IssueDate {value:?}");
    Ok(date)
}

pub fn from_rows(start: NaiveDate, end: NaiveDate, sales: Vec<Value>, purchases: Vec<Value>) -> Result<InvoiceHistory> {
    validate_range(start, end)?;
    let mut months = BTreeMap::<NaiveDate, InvoiceMonth>::new();
    for (kind, rows) in [(InvoiceType::Sales, sales), (InvoiceType::Purchases, purchases)] {
        let mut dated_rows = rows.into_iter().map(|invoice| {
            let date = issue_date(&invoice)?;
            ensure!((start..=end).contains(&date), "Invoice IssueDate {date} is outside history range {start}..{end}");
            Ok((date, invoice))
        }).collect::<Result<Vec<_>>>()?;
        dated_rows.sort_by_cached_key(|(date, invoice)| (
            *date,
            invoice["InvoiceBody"]["InvoiceNumber"].as_str().unwrap_or("").to_string(),
            invoice.to_string(),
        ));
        for (date, invoice) in dated_rows {
            let month = date.with_day(1).unwrap();
            let entry = months.entry(month).or_insert_with(|| InvoiceMonth {
                month, sales: Vec::new(), purchases: Vec::new(),
            });
            match kind {
                InvoiceType::Sales => entry.sales.push(invoice),
                InvoiceType::Purchases => entry.purchases.push(invoice),
            }
        }
    }
    let months: Vec<_> = months.into_values().collect();
    for month in &months { month.totals()?; }
    Ok(InvoiceHistory { months, start, end })
}

const CACHE_VERSION: u32 = 1;
const CACHE_TTL: u64 = 3600;
static CACHE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Serialize, Deserialize)]
struct CachedWindow {
    version: u32,
    kind: String,
    start: String,
    end: String,
    timestamp: u64,
    rows: Vec<Value>,
}

struct HistoryCache {
    directory: PathBuf,
}

fn timestamp() -> Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH).context("System clock is before the Unix epoch")?.as_secs())
}

fn unique_suffix() -> Result<String> {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).context("System clock is before the Unix epoch")?.as_nanos();
    Ok(format!("{}-{nanos}-{}", std::process::id(), CACHE_SEQUENCE.fetch_add(1, Ordering::Relaxed)))
}

impl HistoryCache {
    fn new(root: &Path, endpoint: &str, user_id: &str) -> Self {
        let mut hash = Sha256::new();
        hash.update((endpoint.len() as u64).to_be_bytes());
        hash.update(endpoint.as_bytes());
        hash.update(user_id.as_bytes());
        let scope: String = hash.finalize().iter().map(|byte| format!("{byte:02x}")).collect();
        Self { directory: root.join(scope) }
    }

    fn for_user(user: &AppUser) -> Result<Self> {
        let home = std::env::var_os("USERPROFILE").filter(|home| !home.is_empty())
            .context("USERPROFILE is not set; cannot locate the invoice history cache")?;
        let root = PathBuf::from(home).join(".ksefbot").join("cache").join("invoice-history");
        Ok(Self::new(&root, cf_worker_url!(), &user.id))
    }

    fn path(&self, kind: InvoiceType, start: NaiveDate, end: NaiveDate) -> PathBuf {
        self.directory.join(format!("{kind}-{start}_{end}.json"))
    }

    fn read(&self, kind: InvoiceType, start: NaiveDate, end: NaiveDate, now: u64) -> Result<Option<Vec<Value>>> {
        let path = self.path(kind, start, end);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error).with_context(|| format!("Could not read invoice cache {}", path.display())),
        };
        let validate = || -> Result<CachedWindow> {
            let cached: CachedWindow = serde_json::from_slice(&bytes)?;
            ensure!(cached.version == CACHE_VERSION, "Unsupported cache version {}", cached.version);
            ensure!(cached.kind == kind.to_string() && cached.start == start.to_string() && cached.end == end.to_string(),
                "Cache window or invoice type does not match the requested window");
            ensure!(cached.timestamp <= now, "Cache timestamp is in the future");
            validate_rows(kind, start, end, &cached.rows)?;
            Ok(cached)
        };
        let cached = validate().with_context(|| format!(
            "Invalid invoice cache {}; remove this file and retry", path.display()))?;
        if now - cached.timestamp >= CACHE_TTL { return Ok(None); }
        Ok(Some(cached.rows))
    }

    fn write(&self, kind: InvoiceType, start: NaiveDate, end: NaiveDate, now: u64, rows: &[Value]) -> Result<()> {
        let path = self.path(kind, start, end);
        fs::create_dir_all(&self.directory)
            .with_context(|| format!("Could not create invoice cache directory {}", self.directory.display()))?;
        let bytes = serde_json::to_vec(&CachedWindow {
            version: CACHE_VERSION, kind: kind.to_string(), start: start.to_string(), end: end.to_string(),
            timestamp: now, rows: rows.to_vec(),
        })?;
        let temporary = path.with_extension(format!("json.{}.tmp", unique_suffix()?));
        let mut file = OpenOptions::new().write(true).create_new(true).open(&temporary)
            .with_context(|| format!("Could not create invoice cache temporary file {}", temporary.display()))?;
        let result = (|| -> Result<()> {
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temporary, &path)?;
            Ok(())
        })().with_context(|| format!("Could not write invoice cache {}", path.display()));
        if result.is_err() {
            fs::remove_file(&temporary)
                .with_context(|| format!("Could not clean up invoice cache temporary file {}", temporary.display()))?;
        }
        result
    }

    async fn fetch<F, Fut>(&self, kind: InvoiceType, start: NaiveDate, end: NaiveDate, now: u64, fetch: F) -> Result<Vec<Value>>
    where F: FnOnce() -> Fut, Fut: Future<Output = Result<Vec<Value>>> {
        if let Some(rows) = self.read(kind, start, end, now)? { return Ok(rows); }
        let rows = fetch().await.with_context(|| format!("Could not load {kind} from {start} to {end}"))?;
        validate_rows(kind, start, end, &rows)?;
        self.write(kind, start, end, now, &rows)?;
        Ok(rows)
    }

    fn invalidate_sales(&self) -> Result<()> {
        let entries = match fs::read_dir(&self.directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error).with_context(|| format!("Could not list invoice cache {}", self.directory.display())),
        };
        for entry in entries {
            let entry = entry.with_context(|| format!("Could not read invoice cache entry in {}", self.directory.display()))?;
            let name = entry.file_name();
            let Some(window) = name.to_str().and_then(|name| name.strip_prefix("sales-")).and_then(|name| name.strip_suffix(".json")) else { continue; };
            let Some((start, end)) = window.split_once('_') else { continue; };
            let (Ok(start), Ok(end)) = (NaiveDate::parse_from_str(start, "%Y-%m-%d"), NaiveDate::parse_from_str(end, "%Y-%m-%d")) else { continue; };
            if entry.path() != self.path(InvoiceType::Sales, start, end) { continue; }
            match fs::remove_file(entry.path()) {
                Ok(()) => {}
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(error) => return Err(error).with_context(|| format!("Could not invalidate invoice cache {}", entry.path().display())),
            }
        }
        Ok(())
    }
}

fn validate_rows(kind: InvoiceType, start: NaiveDate, end: NaiveDate, rows: &[Value]) -> Result<()> {
    let (sales, purchases) = match kind {
        InvoiceType::Sales => (rows.to_vec(), Vec::new()),
        InvoiceType::Purchases => (Vec::new(), rows.to_vec()),
    };
    from_rows(start, end, sales, purchases)?;
    Ok(())
}

async fn fetch_history<F, Fut>(cache: &HistoryCache, start: NaiveDate, end: NaiveDate, now: u64, fetch: F) -> Result<InvoiceHistory>
where F: Fn(InvoiceType, NaiveDate, NaiveDate) -> Fut, Fut: Future<Output = Result<Vec<Value>>> {
    let mut sales = Vec::new();
    let mut purchases = Vec::new();
    for (from, to) in query_windows(start, end)?.into_iter().rev() {
        let (window_sales, window_purchases) = tokio::join!(
            cache.fetch(InvoiceType::Sales, from, to, now, || fetch(InvoiceType::Sales, from, to)),
            cache.fetch(InvoiceType::Purchases, from, to, now, || fetch(InvoiceType::Purchases, from, to)),
        );
        sales.extend(window_sales?);
        purchases.extend(window_purchases?);
    }
    from_rows(start, end, sales, purchases)
}

pub async fn load(user: &AppUser, start: NaiveDate, end: NaiveDate) -> Result<InvoiceHistory> {
    validate_range(start, end)?;
    let cache = HistoryCache::for_user(user)?;
    fetch_history(&cache, start, end, timestamp()?, |kind, from, to| async move {
        list_invoices(user, &kind, &from.format("%Y/%m/%d").to_string(), &to.format("%Y/%m/%d").to_string()).await
    }).await
}

pub fn invalidate_sales(user: &AppUser) -> Result<()> {
    HistoryCache::for_user(user)?.invalidate_sales()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::cell::RefCell;
    use std::future::ready;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::current_dir().unwrap().join(format!(".history-cache-test-{}", unique_suffix().unwrap()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn cache(&self, endpoint: &str, user: &str) -> HistoryCache {
            HistoryCache::new(&self.0, endpoint, user)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    fn date(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }

    fn row(day: &str, number: &str, currency: &str, amount: f64) -> Value {
        json!({"InvoiceBody": {
            "IssueDate": day, "InvoiceNumber": number,
            "CurrencyCode": currency, "TotalGrossAmount": amount,
        }})
    }

    #[test]
    fn initial_range_includes_three_calendar_months() {
        assert_eq!(initial_range(date("2026-10-02")), (date("2026-08-01"), date("2026-10-02")));
        assert_eq!(initial_range(date("2024-02-29")), (date("2023-12-01"), date("2024-02-29")));
        assert_eq!(initial_range(date("2025-01-31")), (date("2024-11-01"), date("2025-01-31")));
    }

    #[test]
    fn older_range_covers_previous_three_months() {
        assert_eq!(older_range(date("2026-08-01")), Some((date("2026-05-01"), date("2026-07-31"))));
        assert_eq!(older_range(date("2024-03-01")), Some((date("2023-12-01"), date("2024-02-29"))));
        assert_eq!(older_range(date("0001-04-01")), Some((date("0001-01-01"), date("0001-03-31"))));
        assert_eq!(older_range(date("0001-03-01")), None);
        assert_eq!(older_range(date("0001-01-01")), None);
    }

    #[tokio::test]
    async fn cache_survives_new_instances_and_isolates_users_endpoints_types_and_windows() {
        let directory = TestDirectory::new();
        let calls = RefCell::new(Vec::new());
        let fetch = |kind: InvoiceType, start: NaiveDate, end| {
            calls.borrow_mut().push((kind, start, end));
            ready(Ok(vec![row(&start.to_string(), &kind.to_string(), "PLN", 1.0)]))
        };
        let start = date("2026-08-01");
        let end = date("2026-10-02");
        let cache = directory.cache("https://example.invalid/api", "user/one");
        let first = fetch_history(&cache, start, end, 10_000, &fetch).await.unwrap();
        let second = fetch_history(&cache, start, end, 10_001, &fetch).await.unwrap();
        let restarted = directory.cache("https://example.invalid/api", "user/one");
        let third = fetch_history(&restarted, start, end, 10_002, &fetch).await.unwrap();
        assert_eq!(calls.borrow().len(), 2);
        assert_eq!(first.months[0].sales, second.months[0].sales);
        assert_eq!(first.months[0].purchases, third.months[0].purchases);
        assert_ne!(first.months[0].sales, first.months[0].purchases);
        for (endpoint, user) in [
            ("https://example.invalid/api", "user/two"),
            ("https://other.invalid/api", "user/one"),
        ] {
            fetch_history(&directory.cache(endpoint, user), start, end, 10_002, &fetch).await.unwrap();
        }
        fetch_history(&cache, date("2026-05-01"), date("2026-07-31"), 10_002, &fetch).await.unwrap();
        assert_eq!(calls.borrow().len(), 10);
        let scope = cache.directory.file_name().unwrap().to_str().unwrap();
        assert_eq!(scope.len(), 64);
        assert!(scope.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_ne!(directory.cache("ab", "c").directory, directory.cache("a", "bc").directory);
    }

    #[tokio::test]
    async fn cache_keeps_empty_successes_and_expires_at_one_hour() {
        let directory = TestDirectory::new();
        let calls = RefCell::new(0);
        let fetch = |_, _, _| {
            *calls.borrow_mut() += 1;
            ready(Ok(Vec::new()))
        };
        let start = date("2020-01-01");
        let end = date("2020-01-31");
        let cache = directory.cache("endpoint", "user");
        assert!(fetch_history(&cache, start, end, 10_000, &fetch).await.unwrap().months.is_empty());
        fetch_history(&directory.cache("endpoint", "user"), start, end, 13_599, &fetch).await.unwrap();
        assert_eq!(*calls.borrow(), 2);
        fetch_history(&cache, start, end, 13_600, &fetch).await.unwrap();
        assert_eq!(*calls.borrow(), 4);
        fetch_history(&cache, start, end, 13_601, &fetch).await.unwrap();
        assert_eq!(*calls.borrow(), 4);
        let cached: CachedWindow = serde_json::from_slice(&fs::read(cache.path(InvoiceType::Sales, start, end)).unwrap()).unwrap();
        assert_eq!(cached.timestamp, 13_600);
        assert!(cached.rows.is_empty());
        assert_eq!(fs::read_dir(&cache.directory).unwrap().count(), 2);
    }

    #[tokio::test]
    async fn failures_are_not_cached_and_successful_types_and_windows_survive_retry() {
        for failed_kind in [InvoiceType::Sales, InvoiceType::Purchases] {
            let directory = TestDirectory::new();
            let cache = directory.cache("endpoint", "user");
            let start = date("2025-11-01");
            let end = date("2026-01-31");
            let calls = RefCell::new(Vec::new());
            let error = fetch_history(&cache, start, end, 10_000, |kind, from, to| {
                calls.borrow_mut().push((kind, from, to));
                ready(if kind == failed_kind && from == start {
                    Err(anyhow::anyhow!("Backend unavailable"))
                } else { Ok(Vec::new()) })
            }).await.unwrap_err();
            assert!(format!("{error:#}").contains("Backend unavailable"));
            assert_eq!(calls.borrow().len(), 4);
            let failed_path = cache.path(failed_kind, start, date("2026-01-30"));
            assert!(!failed_path.exists());
            fetch_history(&directory.cache("endpoint", "user"), start, end, 10_001, |kind, from, to| {
                calls.borrow_mut().push((kind, from, to));
                ready(Ok(Vec::new()))
            }).await.unwrap();
            assert_eq!(calls.borrow().len(), 5);
            assert_eq!(calls.borrow().last().unwrap(), &(failed_kind, start, date("2026-01-30")));
            assert!(failed_path.is_file());
        }
    }

    #[tokio::test]
    async fn invalid_backend_rows_are_rejected_without_caching() {
        let directory = TestDirectory::new();
        let cache = directory.cache("endpoint", "user");
        let start = date("2026-01-01");
        let end = date("2026-01-31");
        for invoice in [
            json!({}),
            row("2026-02-01", "outside", "PLN", 1.0),
            row("2026-01-01", "currency", "", 1.0),
            json!({"InvoiceBody": {"IssueDate": "2026-01-01", "CurrencyCode": "PLN", "TotalGrossAmount": "NaN"}}),
        ] {
            let result = cache.fetch(InvoiceType::Sales, start, end, 10_000, || ready(Ok(vec![invoice]))).await;
            assert!(result.is_err());
            assert!(!cache.path(InvoiceType::Sales, start, end).exists());
        }
    }

    #[tokio::test]
    async fn corrupt_cache_is_explicit_and_never_falls_back_to_backend() {
        let directory = TestDirectory::new();
        let cache = directory.cache("endpoint", "user");
        let start = date("2026-01-01");
        let end = date("2026-01-31");
        cache.write(InvoiceType::Sales, start, end, 10_000, &[]).unwrap();
        let path = cache.path(InvoiceType::Sales, start, end);
        let valid: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let mut invalid = vec![b"not json".to_vec()];
        for (field, value) in [
            ("version", json!(CACHE_VERSION + 1)),
            ("start", json!("2025-01-01")),
            ("end", json!("2026-02-01")),
            ("kind", json!("purchases")),
            ("timestamp", json!(10_001)),
            ("rows", json!([{}])),
            ("rows", json!([row("2026-02-01", "outside", "PLN", 1.0)])),
            ("rows", json!([row("2026-01-01", "currency", "", 1.0)])),
        ] {
            let mut value_to_write = valid.clone();
            value_to_write[field] = value;
            invalid.push(serde_json::to_vec(&value_to_write).unwrap());
        }
        for bytes in invalid {
            fs::write(&path, bytes).unwrap();
            let error = cache.fetch(InvoiceType::Sales, start, end, 10_000, || async {
                panic!("Corrupt cache must not trigger a fetch")
            }).await.unwrap_err();
            assert!(error.to_string().contains(&path.display().to_string()));
            assert!(error.to_string().contains("remove this file and retry"));
        }
        assert!(cache.read(InvoiceType::Sales, start, end, 20_000).is_err());
    }

    #[tokio::test]
    async fn filesystem_errors_are_explicit_and_failed_atomic_writes_clean_only_their_temp_file() {
        let directory = TestDirectory::new();
        let cache = directory.cache("endpoint", "user");
        let start = date("2026-01-01");
        let end = date("2026-01-31");
        let path = cache.path(InvoiceType::Sales, start, end);
        fs::create_dir_all(&path).unwrap();
        let other_temp = cache.directory.join("other-writer.tmp");
        fs::write(&other_temp, b"keep").unwrap();
        let error = cache.fetch(InvoiceType::Sales, start, end, 10_000, || async {
            panic!("Filesystem errors must not trigger a fetch")
        }).await.unwrap_err();
        assert!(error.to_string().contains("Could not read invoice cache"));
        assert!(error.to_string().contains(&path.display().to_string()));
        let error = cache.write(InvoiceType::Sales, start, end, 10_000, &[]).unwrap_err();
        assert!(error.to_string().contains("Could not write invoice cache"));
        assert!(path.is_dir());
        assert_eq!(fs::read(&other_temp).unwrap(), b"keep");
        assert_eq!(fs::read_dir(&cache.directory).unwrap().count(), 2);
        let error = cache.invalidate_sales().unwrap_err();
        assert!(error.to_string().contains("Could not invalidate invoice cache"));
    }

    #[tokio::test]
    async fn sales_invalidation_preserves_purchases_other_scopes_and_unrelated_files() {
        let directory = TestDirectory::new();
        let cache = directory.cache("endpoint", "user");
        let other = directory.cache("endpoint", "other");
        let start = date("2026-01-01");
        let end = date("2026-01-31");
        cache.invalidate_sales().unwrap();
        for cache in [&cache, &other] {
            fetch_history(cache, start, end, 10_000, |_, _, _| ready(Ok(Vec::new()))).await.unwrap();
        }
        cache.write(InvoiceType::Sales, date("2025-01-01"), date("2025-01-31"), 10_000, &[]).unwrap();
        for name in ["sales-unrelated.json", "sales-2026-01-01_2026-01-31.json.other.tmp", "notes.txt"] {
            fs::write(cache.directory.join(name), b"keep").unwrap();
        }
        cache.invalidate_sales().unwrap();
        cache.invalidate_sales().unwrap();
        assert_eq!(fs::read_dir(&cache.directory).unwrap().count(), 4);
        assert!(cache.read(InvoiceType::Sales, start, end, 10_000).unwrap().is_none());
        assert!(cache.read(InvoiceType::Purchases, start, end, 10_000).unwrap().is_some());
        assert!(other.read(InvoiceType::Sales, start, end, 10_000).unwrap().is_some());
        let calls = RefCell::new(Vec::new());
        fetch_history(&cache, start, end, 10_001, |kind, _, _| {
            calls.borrow_mut().push(kind);
            ready(Ok(Vec::new()))
        }).await.unwrap();
        assert_eq!(*calls.borrow(), vec![InvoiceType::Sales]);
    }

    #[test]
    fn groups_only_nonempty_months_and_sorts_all_invoices() {
        let a = row("2024-01-04T12:30:00Z", "A", "PLN", 10.0);
        let b = row("2024-01-04", "B", "PLN", 20.0);
        let c = row("2024-01-05", "C", "PLN", 30.0);
        let purchase = row("2024-03-01", "P", "EUR", 4.0);
        let history = from_rows(date("2024-01-01"), date("2024-12-31"),
            vec![c.clone(), b.clone(), a.clone()], vec![purchase.clone()]).unwrap();
        assert_eq!(history.start, date("2024-01-01"));
        assert_eq!(history.end, date("2024-12-31"));
        assert_eq!(history.months.iter().map(|month| month.month).collect::<Vec<_>>(),
            vec![date("2024-01-01"), date("2024-03-01")]);
        assert_eq!(history.months[0].invoices(InvoiceType::Sales), &[a, b, c]);
        assert!(history.months[0].invoices(InvoiceType::Purchases).is_empty());
        assert!(history.months[1].invoices(InvoiceType::Sales).is_empty());
        assert_eq!(history.months[1].invoices(InvoiceType::Purchases), &[purchase]);
    }

    #[test]
    fn sorts_purchases_and_preserves_duplicate_records() {
        let a = row("2024-01-04", "A", "PLN", 10.0);
        let b = row("2024-01-04", "B", "PLN", 20.0);
        let history = from_rows(date("2024-01-01"), date("2024-01-31"), vec![],
            vec![b.clone(), a.clone(), a.clone()]).unwrap();
        assert_eq!(history.months[0].purchases, vec![a.clone(), a, b]);
        assert_eq!(history.months[0].totals().unwrap()[0].purchases, 40.0);
    }

    #[test]
    fn separates_currencies_and_preserves_negative_corrections() {
        let history = from_rows(date("2024-01-01"), date("2024-01-31"), vec![
            row("2024-01-01", "A", "PLN", 100.0),
            row("2024-01-02", "B", "EUR", 20.0),
            row("2024-01-03", "C", "PLN", -150.0),
        ], vec![
            row("2024-01-01", "D", "EUR", -30.0),
            row("2024-01-02", "E", "USD", 5.0),
        ]).unwrap();
        assert_eq!(history.months[0].totals().unwrap(), vec![
            MonthlyTotal { currency: "EUR".into(), sales: 20.0, purchases: -30.0 },
            MonthlyTotal { currency: "PLN".into(), sales: -50.0, purchases: 0.0 },
            MonthlyTotal { currency: "USD".into(), sales: 0.0, purchases: 5.0 },
        ]);
    }

    #[test]
    fn empty_rows_produce_no_months() {
        let history = from_rows(date("2024-01-01"), date("2024-12-31"), vec![], vec![]).unwrap();
        assert!(history.months.is_empty());
    }

    #[test]
    fn rejects_malformed_dates_on_both_sides() {
        for value in [Value::Null, json!(12), json!(""), json!("2024-02-30"),
            json!("2024-1-01"), json!("2024-01-01junk"), json!("0000-01-01"),
            json!("not a date"), json!("2024-01-😀")] {
            let mut invoice = row("2024-01-01", "BAD", "PLN", 1.0);
            invoice["InvoiceBody"]["IssueDate"] = value;
            for kind in [InvoiceType::Sales, InvoiceType::Purchases] {
                let (sales, purchases) = match kind {
                    InvoiceType::Sales => (vec![invoice.clone()], vec![]),
                    InvoiceType::Purchases => (vec![], vec![invoice.clone()]),
                };
                let error = from_rows(date("2024-01-01"), date("2024-12-31"), sales, purchases).unwrap_err();
                assert!(error.to_string().contains("IssueDate"), "{error}");
            }
        }
        assert!(from_rows(date("2024-01-01"), date("2024-12-31"), vec![json!({})], vec![]).is_err());
    }

    #[test]
    fn rejects_missing_invalid_and_nonfinite_amounts_and_currencies() {
        for (field, value) in [
            ("TotalGrossAmount", Value::Null),
            ("TotalGrossAmount", json!("NaN")),
            ("TotalGrossAmount", json!("Infinity")),
            ("TotalGrossAmount", json!("1.00")),
            ("TotalGrossAmount", json!(true)),
            ("CurrencyCode", Value::Null),
            ("CurrencyCode", json!("")),
            ("CurrencyCode", json!("  ")),
            ("CurrencyCode", json!(123)),
        ] {
            let mut invoice = row("2024-01-01", "BAD", "PLN", 1.0);
            invoice["InvoiceBody"][field] = value;
            let error = from_rows(date("2024-01-01"), date("2024-01-31"), vec![], vec![invoice]).unwrap_err();
            assert!(error.to_string().contains(field), "{error}");
        }
        for field in ["CurrencyCode", "TotalGrossAmount"] {
            let mut invoice = row("2024-01-01", "BAD", "PLN", 1.0);
            invoice["InvoiceBody"].as_object_mut().unwrap().remove(field);
            assert!(from_rows(date("2024-01-01"), date("2024-01-31"), vec![invoice], vec![]).is_err());
        }
        let month = InvoiceMonth {
            month: date("2024-01-01"),
            sales: vec![row("2024-01-01", "A", "PLN", f64::MAX), row("2024-01-01", "B", "PLN", f64::MAX)],
            purchases: vec![],
        };
        assert!(month.totals().unwrap_err().to_string().contains("Nonfinite"));
    }

    #[test]
    fn rejects_rows_outside_the_requested_range() {
        assert!(from_rows(date("2024-01-02"), date("2024-01-31"),
            vec![row("2024-01-01", "A", "PLN", 1.0)], vec![]).is_err());
    }

    #[test]
    fn windows_handle_partial_months_year_boundaries_and_leap_days() {
        assert_eq!(query_windows(date("2023-12-15"), date("2024-07-04")).unwrap(), vec![
            (date("2023-12-15"), date("2024-02-29")),
            (date("2024-03-01"), date("2024-05-30")),
            (date("2024-05-31"), date("2024-07-04")),
        ]);
        assert_eq!(query_windows(date("2025-01-31"), date("2025-04-01")).unwrap(), vec![
            (date("2025-01-31"), date("2025-03-31")),
            (date("2025-04-01"), date("2025-04-01")),
        ]);
        assert_eq!(query_windows(date("9999-12-31"), date("9999-12-31")).unwrap(),
            vec![(date("9999-12-31"), date("9999-12-31"))]);
    }

    #[test]
    fn windows_have_at_most_three_calendar_months_and_no_gaps_or_overlap() {
        for (start, end) in [
            (date("0001-01-01"), date("0002-01-01")),
            (date("2023-11-30"), date("2025-04-07")),
            (date("2024-02-29"), date("2024-02-29")),
            (date("9998-10-12"), date("9999-12-31")),
        ] {
            let windows = query_windows(start, end).unwrap();
            assert_eq!(windows.first().unwrap().0, start);
            assert_eq!(windows.last().unwrap().1, end);
            let mut covered = 0;
            for &(from, to) in &windows {
                assert!(from <= to);
                assert!((to - from).num_days() <= 90);
                let months = (to.year() - from.year()) * 12 + to.month() as i32 - from.month() as i32 + 1;
                assert!((1..=3).contains(&months));
                covered += (to - from).num_days() + 1;
            }
            for pair in windows.windows(2) {
                assert_eq!(pair[0].1.succ_opt().unwrap(), pair[1].0);
            }
            assert_eq!(covered, (end - start).num_days() + 1);
        }
    }

    #[test]
    fn reported_november_to_january_request_is_split_at_ninety_days() {
        assert_eq!(query_windows(date("2025-11-01"), date("2026-01-31")).unwrap(), vec![
            (date("2025-11-01"), date("2026-01-30")),
            (date("2026-01-31"), date("2026-01-31")),
        ]);
        for month in 1..=12 {
            let start = NaiveDate::from_ymd_opt(2025, month, 1).unwrap();
            let end = start.checked_add_months(Months::new(12)).unwrap();
            for (from, to) in query_windows(start, end).unwrap() {
                assert!((to - from).num_days() <= 90);
            }
        }
    }

    #[test]
    fn rejects_reversed_and_out_of_bounds_ranges() {
        for (start, end) in [
            (date("2024-02-01"), date("2024-01-01")),
            (NaiveDate::from_ymd_opt(0, 1, 1).unwrap(), date("2024-01-01")),
            (date("2024-01-01"), NaiveDate::from_ymd_opt(10000, 1, 1).unwrap()),
        ] {
            assert!(query_windows(start, end).is_err());
            assert!(from_rows(start, end, vec![], vec![]).is_err());
        }
    }
}
