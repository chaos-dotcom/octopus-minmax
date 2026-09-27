//! Local-time helpers.  Python's `datetime.now()` and `date.today()` use the process
//! timezone, which the container sets with `TZ`; chrono reads the same value.

use chrono::{DateTime, Datelike, Local, NaiveDate, TimeZone, Utc};

pub fn now() -> DateTime<Local> {
    Local::now()
}

pub fn today() -> NaiveDate {
    Local::now().date_naive()
}

pub fn local_midnight() -> DateTime<Local> {
    let now = Local::now();
    Local
        .with_ymd_and_hms(now.year(), now.month(), now.day(), 0, 0, 0)
        .single()
        .unwrap_or(now)
}

/// `datetime.now().strftime("%d/%m/%Y %H:%M")`
pub fn stamp_day_month_year_minutes() -> String {
    Local::now().format("%d/%m/%Y %H:%M").to_string()
}

/// `date.today().isoformat()`
pub fn today_iso() -> String {
    today().format("%Y-%m-%d").to_string()
}

/// `datetime.now().strftime("%H:%M")`
pub fn hour_minute() -> String {
    Local::now().format("%H:%M").to_string()
}

/// `datetime.now().strftime("%H:%M:%S")`
pub fn hour_minute_second() -> String {
    Local::now().format("%H:%M:%S").to_string()
}

/// `datetime.now().strftime("%Y-%m-%d %H:%M")`
pub fn stamp_year_month_day_minutes() -> String {
    Local::now().format("%Y-%m-%d %H:%M").to_string()
}

/// `datetime.now().strftime("%a %d %b")` - C locale abbreviations.
pub fn weekday_day_month() -> String {
    Local::now().format("%a %d %b").to_string()
}

pub fn unix_seconds() -> u64 {
    Utc::now().timestamp().max(0) as u64
}

/// `datetime.fromisoformat(text.replace("Z", "+00:00"))`
pub fn parse_iso(text: &str) -> Option<DateTime<chrono::FixedOffset>> {
    let normalised = text.replace('Z', "+00:00");
    DateTime::parse_from_rfc3339(&normalised)
        .ok()
        .or_else(|| {
            chrono::NaiveDateTime::parse_from_str(&normalised, "%Y-%m-%dT%H:%M:%S%.f%:z")
                .ok()
                .and_then(|naive| {
                    chrono::FixedOffset::east_opt(0).map(|offset| offset.from_local_datetime(&naive).single())
                })
                .flatten()
        })
}
