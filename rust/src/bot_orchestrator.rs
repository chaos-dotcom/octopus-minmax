//! `src/bot_orchestrator.py`

use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::account_info::AccountInfo;
use crate::account_manager::AccountManager;
use crate::comparison_engine::{ComparisonEngine, ComparisonResult};
use crate::config;
use crate::errors::AppResult;
use crate::notification_service::NotificationService;
use crate::pyhash;
use crate::pyrepr::{fixed, repr_float};
use crate::query_service::QueryService;
use crate::tariff::{self, Tariff};
use crate::{logd, logi};

/// `get_timestamp()`
pub fn get_timestamp() -> String {
    crate::clock::stamp_day_month_year_minutes()
}

/// `random.randint(10, 900)`.  The Python original draws from an unseeded Mersenne
/// Twister, so this value is not reproducible in either implementation.
fn random_delay() -> u32 {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_nanos() as u64)
        .unwrap_or(0)
        ^ (std::process::id() as u64).wrapping_mul(0x9E3779B97F4A7C15);
    let mut state = seed | 1;
    state ^= state << 13;
    state ^= state >> 7;
    state ^= state << 17;
    10 + (state % 891) as u32
}

pub struct BotOrchestrator {
    pub query_service: Option<Arc<QueryService>>,
    pub account_manager: Option<Arc<Mutex<AccountManager>>>,
    pub tariffs: Vec<&'static Tariff>,
    pub last_execution_minute: Option<String>,
    pub notification_service: Option<Arc<NotificationService>>,
}

impl BotOrchestrator {
    pub fn new() -> BotOrchestrator {
        logd!("octobot.bot_orchestrator", "bot_orchestrator.__init__", "Initialising BotOrchestrator");
        BotOrchestrator {
            query_service: None,
            account_manager: None,
            tariffs: Vec::new(),
            last_execution_minute: None,
            notification_service: None,
        }
    }

    /// `BotOrchestrator.start`
    pub fn start(&mut self) {
        let settings = config::get();
        let service = Arc::new(NotificationService::new(
            &settings.notification_urls,
            settings.batch_notifications,
        ));
        self.notification_service = Some(service.clone());

        let mode_msg = if settings.one_off_run {
            "ONE_OFF mode enabled".to_string()
        } else {
            format!("Scheduled mode, running at {}", settings.execution_time)
        };
        service.send_notification(
            &format!(
                "[{}] Octobot {} - {} \n Check port {} for dashboard.",
                get_timestamp(),
                settings.bot_version,
                mode_msg,
                settings.web_port
            ),
            "",
            false,
            true,
        );

        loop {
            let settings = config::get();
            if settings.one_off_run && !settings.one_off_executed {
                service.send_notification(
                    &format!(
                        "[{}] Octobot {} - Running one-off comparison",
                        get_timestamp(),
                        settings.bot_version
                    ),
                    "",
                    false,
                    true,
                );
                self.run_tariff_compare();
                config::update(|state| state.one_off_executed = true);
            } else if !settings.one_off_run {
                let current_time = crate::clock::hour_minute();
                let current_minute = crate::clock::stamp_year_month_day_minutes();
                if current_time == settings.execution_time
                    && self.last_execution_minute.as_deref() != Some(current_minute.as_str())
                {
                    self.last_execution_minute = Some(current_minute);
                    let delay = random_delay();
                    service.send_notification(
                        &format!(
                            "[{}] Octobot {} - Initiating comparison in {:.1} minutes",
                            get_timestamp(),
                            settings.bot_version,
                            delay as f64 / 60.0
                        ),
                        "",
                        false,
                        true,
                    );
                    std::thread::sleep(Duration::from_secs(delay as u64));
                    self.run_tariff_compare();
                }
            }
            std::thread::sleep(Duration::from_secs(30));
        }
    }

    fn initialize(&mut self) -> AppResult<()> {
        logd!("octobot.bot_orchestrator", "bot_orchestrator._initialize", "bot_orchestrator");
        let settings = config::get();
        self.load_tariffs_from_ids(&settings.tariffs);
        let service = Arc::new(QueryService::new(&settings.api_key, &settings.base_url)?);
        self.query_service = Some(service.clone());
        self.account_manager = Some(AccountManager::get_instance(service, self.tariffs.clone()));
        Ok(())
    }

    /// `BotOrchestrator._load_tariffs_from_ids`
    fn load_tariffs_from_ids(&mut self, tariff_ids: &str) {
        let requested: Vec<String> = tariff_ids.to_lowercase().split(',').map(|item| item.to_string()).collect();
        let (k0, k1) = pyhash::secret_from_env(std::env::var("PYTHONHASHSEED").ok().as_deref());
        let requested_ids = pyhash::set_order(&requested, k0, k1);
        logd!(
            "octobot.bot_orchestrator",
            "bot_orchestrator._load_tariffs_from_ids",
            " Requested tariff IDs - {{{}}}",
            requested_ids
                .iter()
                .map(|id| crate::pyrepr::repr_str(id))
                .collect::<Vec<_>>()
                .join(", ")
        );
        let mut matched_tariffs = Vec::new();
        for tariff_id in &requested_ids {
            match tariff::find(tariff_id) {
                Some(found) => matched_tariffs.push(found),
                None => {
                    if let Some(service) = &self.notification_service {
                        service.send_notification(
                            &format!("Warning: No tariff found for ID '{}'", tariff_id),
                            "",
                            false,
                            true,
                        );
                    }
                }
            }
        }
        self.tariffs = matched_tariffs;
    }

    /// `BotOrchestrator._run_tariff_compare`
    fn run_tariff_compare(&mut self) {
        let service = self.notification_service.clone().expect("notification service");
        let outcome = (|| -> AppResult<()> {
            self.initialize()?;
            if self.query_service.is_none() {
                return Err(crate::errors::AppError::new("ERROR: QueryService initialization failed"));
            }
            self.compare_and_switch()
        })();
        if let Err(error) = outcome {
            service.send_notification(&error.to_string(), "Octobot Error", true, true);
        }
        if config::get().batch_notifications {
            service.send_batch_notification();
        }
    }

    /// `BotOrchestrator._format_comparison_summary`
    fn format_comparison_summary(&self, result: &ComparisonResult) -> String {
        let mut lines: Vec<String> = Vec::new();
        let current = &result.current_tariff_comparison;
        if let Some(breakdown) = &current.cost_breakdown {
            lines.push(format!(
                "Total Consumption today: {:.4} kWh",
                breakdown.total_kwh
            ));
            lines.push(format!(
                "Current tariff {}: £{:.2} (£{:.2} con + £{:.2} s/c)",
                current.tariff.display_name,
                breakdown.total_cost_pounds(),
                breakdown.consumption_cost_pounds(),
                breakdown.standing_charge_pounds()
            ));
        }
        for comparison in &result.alternative_comparisons {
            match &comparison.cost_breakdown {
                Some(breakdown) if comparison.is_valid() => {
                    lines.push(format!(
                        "Potential cost on {}: £{:.2} (£{:.2} con + £{:.2} s/c)",
                        comparison.tariff.display_name,
                        breakdown.total_cost_pounds(),
                        breakdown.consumption_cost_pounds(),
                        breakdown.standing_charge_pounds()
                    ));
                }
                _ => lines.push(format!("No cost for {}", comparison.tariff.display_name)),
            }
        }
        lines.join("\n")
    }

    /// `BotOrchestrator._compare_and_switch`
    fn compare_and_switch(&mut self) -> AppResult<()> {
        let settings = config::get();
        let service = self.notification_service.clone().expect("notification service");
        let welcome = if settings.dry_run {
            "DRY RUN: Starting comparison of today's costs..."
        } else {
            "Starting comparison of today's costs..."
        };
        service.send_notification(welcome, "", false, true);

        let manager = self.account_manager.clone().expect("account manager");
        let account_info = manager.lock().unwrap().fetch_current_account_info()?;

        let query_service = self.query_service.clone().expect("query service");
        let engine = ComparisonEngine::new(query_service);
        let results = engine.compare_tariffs(&account_info, &self.tariffs)?;

        service.send_notification(&self.format_comparison_summary(&results), "", false, true);

        if results.should_switch() {
            let cheapest = results.cheapest_tariff.expect("cheapest tariff");
            service.send_notification(
                &format!("Initiating Switch to {}", cheapest.display_name),
                "",
                false,
                true,
            );
            if config::get().dry_run {
                service.send_notification("DRY RUN: Not going through with switch today.", "", false, true);
            } else {
                self.execute_switch(cheapest, &account_info)?;
            }
        } else {
            let message = match results.cheapest_tariff {
                None => "Not switching today - no valid alternative tariffs to compare against.".to_string(),
                Some(cheapest) if cheapest == results.current_tariff_comparison.tariff => format!(
                    "You are already on the cheapest tariff: {} at £{:.2}",
                    cheapest.display_name,
                    results
                        .current_tariff_comparison
                        .cost_breakdown
                        .as_ref()
                        .map(|breakdown| breakdown.total_cost_pounds())
                        .unwrap_or(0.0)
                ),
                Some(cheapest) => format!(
                    "Not switching today - savings of (£{:.2}) on the cheapest tariff {} are below your threshold of £{:.2}",
                    results.potential_savings / 100.0,
                    cheapest.display_name,
                    config::get().switch_threshold as f64 / 100.0
                ),
            };
            service.send_notification(&message, "", false, true);
        }
        Ok(())
    }

    /// `BotOrchestrator._execute_switch`
    fn execute_switch(&self, target: &'static Tariff, account_info: &AccountInfo) -> AppResult<()> {
        let service = self.notification_service.clone().expect("notification service");
        let manager = self.account_manager.clone().expect("account manager");
        let product_code = target.get_product_code();
        if product_code.is_none() {
            service.send_notification("ERROR: product_code is missing.", "", false, true);
        }
        let enrolment_id = manager
            .lock()
            .unwrap()
            .initiate_tariff_switch(product_code.as_deref().unwrap_or(""))?;
        let enrolment_id = match enrolment_id {
            Some(value) => value,
            None => {
                service.send_notification("ERROR: Couldn't get enrolment ID", "", false, true);
                return Ok(());
            }
        };

        let wait_time = 120u64;
        service.send_notification(
            &format!(
                "Tariff switch requested successfully. Waiting {}s before attempting to accept new agreement.",
                wait_time
            ),
            "",
            false,
            true,
        );
        std::thread::sleep(Duration::from_secs(wait_time));

        let accepted_version = manager
            .lock()
            .unwrap()
            .accept_new_agreement(product_code.as_deref().unwrap_or(""), &enrolment_id)?;
        service.send_notification(
            &format!("Accepted agreement (v.{}). Switch successful.", accepted_version),
            "",
            false,
            true,
        );

        // Exactly as the reference: nothing is sent when the first verification passes.
        let verified = manager.lock().unwrap().verify_new_agreement_status()?;
        if !verified {
            service.send_notification("Verification failed, waiting 20 seconds and trying again...", "", false, true);
            std::thread::sleep(Duration::from_secs(60));
            let retried = manager.lock().unwrap().verify_new_agreement_status()?;
            if retried {
                service.send_notification("Verified new agreement successfully. Process finished.", "", false, true);
            } else {
                service.send_notification(
                    &format!(
                        "Unable to verify new agreement after retry. Please check your account and emails.\n\
                         https://octopus.energy/dashboard/new/accounts/{}/messages",
                        config::get().acc_number
                    ),
                    "",
                    false,
                    true,
                );
            }
        }
        let _ = account_info;
        Ok(())
    }
}

#[allow(dead_code)]
fn helpers_used(value: f64) -> (String, String) {
    (fixed(value, 2), repr_float(value))
}

#[allow(dead_code)]
fn unused_log() -> String {
    logi!("octobot.bot_orchestrator", "bot_orchestrator.start", "started");
    String::new()
}
