//! `src/home_assistant_client.py` - today's usage read from a Home Assistant sensor.

use std::time::Duration;

use chrono::{DateTime, Duration as ChronoDuration, NaiveDateTime, TimeZone, Utc};
use serde_json::Value;

use crate::config;
use crate::errors::{AppError, AppResult};
use crate::http;
use crate::pyrepr::repr_str;
use crate::urlcode;
use crate::{logd, logi, logw};

/// `_local_tz()` - the configured zone, or UTC with a warning when it is unknown.
fn local_timezone() -> bool {
    let name = config::get().timezone;
    if name.is_empty() {
        return true;
    }
    let candidate = format!("/usr/share/zoneinfo/{}", name);
    if std::path::Path::new(&candidate).exists() || name == "UTC" {
        return true;
    }
    logw!(
        "octobot.home_assistant",
        "home_assistant_client._local_tz",
        "Could not load timezone '{}' (ZoneInfoNotFoundError); using UTC",
        name
    );
    true
}

pub struct HomeAssistantClient {
    base_url: String,
    token: String,
    import_entity: String,
}

impl HomeAssistantClient {
    pub fn new(base_url: Option<&str>, token: Option<&str>, import_entity: Option<&str>) -> HomeAssistantClient {
        logd!("octobot.home_assistant", "home_assistant_client.__init__", "Initialising HomeAssistantClient");
        let settings = config::get();
        let mut base = base_url.unwrap_or(&settings.ha_url).trim_end_matches('/').to_string();
        if !base.ends_with("/api") {
            base = format!("{}/api", base);
        }
        HomeAssistantClient {
            base_url: base,
            token: token.unwrap_or(&settings.ha_token).to_string(),
            import_entity: import_entity.unwrap_or(&settings.ha_import_entity).to_string(),
        }
    }

    /// `HomeAssistantClient.get_today_half_hourly_consumption`
    pub fn get_today_half_hourly_consumption(&self) -> AppResult<Vec<Value>> {
        if self.token.is_empty() {
            return Err(AppError::new(
                "No Home Assistant token available. Set HA_TOKEN, or run as a Home Assistant \
                 add-on with homeassistant_api enabled.",
            ));
        }
        local_timezone();
        let now_utc = Utc::now();
        let local_now = crate::clock::now();
        let local_midnight = crate::clock::local_midnight();
        let start_utc = DateTime::<Utc>::from_naive_utc_and_offset(
            local_midnight.naive_utc(),
            Utc,
        );
        let end_utc = DateTime::<Utc>::from_naive_utc_and_offset(local_now.naive_utc(), Utc);
        let _ = (now_utc, local_now);

        let mut samples = self.fetch_history(&self.import_entity, start_utc, end_utc)?;
        if samples.is_empty() {
            return Err(AppError::new(format!(
                "Home Assistant returned no history for '{}'. Check HA_IMPORT_ENTITY and that \
                 the entity has history enabled.",
                self.import_entity
            )));
        }
        samples.sort_by(|left, right| left.0.cmp(&right.0));

        let mut slot_start = align_slot_start(start_utc);

        let mut previous_value = value_at(&samples, slot_start);
        let mut slots: Vec<Value> = Vec::new();
        while slot_start < end_utc {
            let slot_end = slot_start + ChronoDuration::minutes(30);
            let end_value = value_at(&samples, slot_end);
            if let (Some(previous), Some(current)) = (previous_value, end_value) {
                let delta_kwh = current - previous;
                if delta_kwh > 0.0 {
                    let mut entry = serde_json::Map::new();
                    entry.insert(
                        "readAt".to_string(),
                        Value::String(slot_end.format("%Y-%m-%dT%H:%M:%SZ").to_string()),
                    );
                    entry.insert(
                        "consumptionDelta".to_string(),
                        serde_json::Number::from_f64(delta_kwh * 1000.0)
                            .map(Value::Number)
                            .unwrap_or(Value::Null),
                    );
                    entry.insert("costDeltaWithTax".to_string(), Value::Null);
                    slots.push(Value::Object(entry));
                }
            }
            previous_value = end_value;
            slot_start = slot_end;
        }

        let total_kwh: f64 = slots
            .iter()
            .map(|slot| slot.get("consumptionDelta").and_then(|value| value.as_f64()).unwrap_or(0.0))
            .sum::<f64>()
            / 1000.0;
        logi!(
            "octobot.home_assistant",
            "home_assistant_client.get_today_half_hourly_consumption",
            "Home Assistant '{}': {} half-hour slots, {:.2} kWh since {}",
            self.import_entity,
            slots.len(),
            total_kwh,
            local_midnight.format("%Y-%m-%d %H:%M")
        );
        Ok(slots)
    }

    /// `HomeAssistantClient._fetch_history`
    fn fetch_history(
        &self,
        entity_id: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> AppResult<Vec<(DateTime<Utc>, f64)>> {
        let start_iso = start.format("%Y-%m-%dT%H:%M:%S+00:00").to_string();
        let end_iso = end.format("%Y-%m-%dT%H:%M:%S%.6f+00:00").to_string();
        let url = format!(
            "{}/history/period/{}",
            self.base_url,
            urlcode::quote(&start_iso)
        );
        let query = urlcode::encode_params(&[
            ("filter_entity_id", entity_id),
            ("end_time", &end_iso),
            ("minimal_response", ""),
        ]);
        logd!(
            "octobot.home_assistant",
            "home_assistant_client._fetch_history",
            "Fetching HA history: {} {{'filter_entity_id': {}, 'end_time': {}, 'minimal_response': ''}}",
            url,
            crate::pyrepr::repr_str(entity_id),
            crate::pyrepr::repr_str(&end_iso)
        );
        let response = http::request(
            "GET",
            &format!("{}?{}", url, query),
            &self.headers(),
            None,
            Duration::from_secs(60),
        )
        .map_err(|error| AppError::new(error.to_string()))?;

        if !response.ok() {
            let text: String = response.text();
            return Err(AppError::new(format!(
                "Home Assistant history request failed: {} {}",
                response.status,
                text.chars().take(200).collect::<String>()
            )));
        }
        let text = response.text();
        let parsed: Value = match serde_json::from_str(&text) {
            Ok(value) => value,
            Err(_) => {
                return Err(AppError::new(format!(
                    "Home Assistant history request returned non-JSON (is HA_URL correct?): {} {}",
                    response.status,
                    text.chars().take(200).collect::<String>()
                )))
            }
        };
        let entries = match parsed.as_array().and_then(|items| items.first()).and_then(|item| item.as_array()) {
            Some(items) => items.clone(),
            None => return Ok(Vec::new()),
        };

        let mut points = Vec::new();
        for entry in entries {
            let state = entry.get("state");
            let state_text = match state {
                None | Some(Value::Null) => continue,
                Some(value) => match value.as_str() {
                    Some(text) if text == "unknown" || text == "unavailable" || text.is_empty() => continue,
                    Some(text) => text,
                    None => continue,
                },
            };
            let value = match state_text.parse::<f64>() {
                Ok(number) => number,
                Err(_) => continue,
            };
            let stamp = match entry.get("last_updated").and_then(|value| value.as_str()) {
                Some(text) => text.to_string(),
                None => match entry.get("last_changed").and_then(|value| value.as_str()) {
                    Some(text) => text.to_string(),
                    None => continue,
                },
            };
            let normalised = stamp.replace('Z', "+00:00");
            let parsed_stamp = match DateTime::parse_from_rfc3339(&normalised) {
                Ok(value) => value.with_timezone(&Utc),
                Err(_) => match NaiveDateTime::parse_from_str(&normalised, "%Y-%m-%dT%H:%M:%S%.f%:z") {
                    Ok(naive) => Utc.from_utc_datetime(&naive),
                    Err(_) => continue,
                },
            };
            points.push((parsed_stamp, value));
        }
        Ok(points)
    }

    fn headers(&self) -> Vec<(String, String)> {
        vec![
            ("User-Agent".to_string(), "octopus-minmax-bot/1.0".to_string()),
            ("Accept-Encoding".to_string(), "gzip, deflate".to_string()),
            ("Accept".to_string(), "application/json".to_string()),
            ("Connection".to_string(), "keep-alive".to_string()),
            ("Authorization".to_string(), format!("Bearer {}", self.token)),
        ]
    }
}

/// `start_utc.replace(second=0, microsecond=0)` then the :00/:30 alignment.
fn align_slot_start(start: DateTime<Utc>) -> DateTime<Utc> {
    use chrono::Timelike;
    let minute = if start.minute() >= 30 { 30 } else { 0 };
    start
        .with_second(0)
        .and_then(|value| value.with_nanosecond(0))
        .and_then(|value| value.with_minute(minute))
        .unwrap_or(start)
}

/// `HomeAssistantClient._value_at`
fn value_at(samples: &[(DateTime<Utc>, f64)], moment: DateTime<Utc>) -> Option<f64> {
    let mut value = None;
    for (timestamp, sample) in samples {
        if *timestamp <= moment {
            value = Some(*sample);
        } else {
            break;
        }
    }
    value
}

#[allow(dead_code)]
fn unused(value: &str) -> String {
    repr_str(value)
}
