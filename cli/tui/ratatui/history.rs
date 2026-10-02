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
