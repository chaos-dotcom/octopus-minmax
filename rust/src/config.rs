//! Runtime configuration, read from the same environment variables as `src/config.py`.

use std::env;
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub bot_version: String,
    pub api_key: String,
    pub acc_number: String,
    pub base_url: String,
    pub notification_urls: String,
    pub batch_notifications: bool,
    pub execution_time: String,
    pub switch_threshold: i64,
    pub tariffs: String,
    pub one_off_run: bool,
    pub one_off_executed: bool,
    pub dry_run: bool,
    pub web_username: String,
    pub web_password: String,
    pub web_port: u16,
    pub consumption_source: String,
    pub ha_url: String,
    pub ha_token: String,
    pub ha_import_entity: String,
    pub timezone: String,
}

fn env_or(name: &str, default: &str) -> String {
    env::var(name).unwrap_or_else(|_| default.to_string())
}

/// `os.getenv(name, "false") in ["true", "True", "1"]`
fn env_flag(name: &str) -> bool {
    matches!(env_or(name, "false").as_str(), "true" | "True" | "1")
}

/// `int(os.getenv(name, default))`
fn env_int(name: &str, default: &str) -> i64 {
    let raw = env_or(name, default);
    match raw.trim().parse::<i64>() {
        Ok(value) => value,
        Err(_) => {
            eprintln!(
                "invalid literal for int() with base 10: {} (environment variable {})",
                crate::pyrepr::repr_str(&raw), name
            );
            std::process::exit(1);
        }
    }
}

impl Config {
    pub fn from_env() -> Config {
        let timezone = env_or("TZ", "Europe/London");
        let ha_token = match env::var("HA_TOKEN") {
            Ok(value) if !value.is_empty() => value,
            _ => env_or("SUPERVISOR_TOKEN", ""),
        };
        Config {
            bot_version: env::var("BOT_VERSION").unwrap_or_else(|_| {
                option_env!("OCTO_BOT_VERSION").unwrap_or("v.local").to_string()
            }),
            api_key: env_or("API_KEY", ""),
            acc_number: env_or("ACC_NUMBER", ""),
            base_url: env_or("BASE_URL", "https://api.octopus.energy/v1"),
            notification_urls: env_or("NOTIFICATION_URLS", ""),
            batch_notifications: env_flag("BATCH_NOTIFICATIONS"),
            execution_time: env_or("EXECUTION_TIME", "23:00"),
            switch_threshold: env_int("SWITCH_THRESHOLD", "2"),
            tariffs: env_or("TARIFFS", "go,agile,flexible"),
            one_off_run: env_flag("ONE_OFF"),
            one_off_executed: false,
            dry_run: env_flag("DRY_RUN"),
            web_username: env_or("WEB_USERNAME", "admin"),
            web_password: env_or("WEB_PASSWORD", "admin"),
            web_port: env_int("WEB_PORT", "5050") as u16,
            consumption_source: env_or("CONSUMPTION_SOURCE", "homeassistant").trim().to_lowercase(),
            ha_url: env_or("HA_URL", "http://supervisor/core/api")
                .trim_end_matches('/')
                .to_string(),
            ha_token,
            ha_import_entity: env_or("HA_IMPORT_ENTITY", "sensor.predbat_givtcp_0_import_total"),
            timezone,
        }
    }
}

static CONFIG: OnceLock<Mutex<Config>> = OnceLock::new();

fn store() -> &'static Mutex<Config> {
    CONFIG.get_or_init(|| Mutex::new(Config::from_env()))
}

/// A snapshot of the current configuration.
pub fn get() -> Config {
    store().lock().unwrap().clone()
}

/// Replace the configuration (used by the web UI, exactly like `config_manager.update_config`).
pub fn replace(config: Config) {
    *store().lock().unwrap() = config;
}

/// Mutate the configuration in place.
pub fn update<F: FnOnce(&mut Config)>(edit: F) {
    let mut guard = store().lock().unwrap();
    edit(&mut guard);
}

/// Force the configuration to be initialised during start-up.
pub fn initialise() {
    let _ = get();
}
