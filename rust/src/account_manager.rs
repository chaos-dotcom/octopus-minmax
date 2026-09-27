//! `src/account_manager.py`

use std::sync::{Arc, Mutex, OnceLock};

use serde_json::Value;

use crate::account_info::AccountInfo;
use crate::clock;
use crate::config;
use crate::errors::{key_error, AppError, AppResult};
use crate::home_assistant_client::HomeAssistantClient;
use crate::queries;
use crate::query_service::{expect_float, QueryService};
use crate::tariff::Tariff;
use crate::{logd, logi, logw};

static INSTANCE: OnceLock<Mutex<Option<Arc<Mutex<AccountManager>>>>> = OnceLock::new();

fn instance_slot() -> &'static Mutex<Option<Arc<Mutex<AccountManager>>>> {
    INSTANCE.get_or_init(|| Mutex::new(None))
}

pub struct AccountManager {
    pub query_service: Arc<QueryService>,
    pub available_tariffs: Vec<&'static Tariff>,
    pub current_account_info: Option<AccountInfo>,
    pub mpan: Option<String>,
    pub device_id: Option<String>,
    pub region_code: Option<String>,
}

impl AccountManager {
    pub fn new(query_service: Arc<QueryService>, available_tariffs: Vec<&'static Tariff>) -> AccountManager {
        logd!("octobot.account_manager", "account_manager.__init__", "Initialising AccountManager");
        AccountManager {
            query_service,
            available_tariffs,
            current_account_info: None,
            mpan: None,
            device_id: None,
            region_code: None,
        }
    }

    /// `AccountManager.get_instance`
    pub fn get_instance(
        query_service: Arc<QueryService>,
        available_tariffs: Vec<&'static Tariff>,
    ) -> Arc<Mutex<AccountManager>> {
        let mut slot = instance_slot().lock().unwrap();
        if let Some(existing) = slot.as_ref() {
            return existing.clone();
        }
        let created = Arc::new(Mutex::new(AccountManager::new(query_service, available_tariffs)));
        *slot = Some(created.clone());
        created
    }

    /// `_get_agreement_terms_version`
    fn get_agreement_terms_version(&self, product_code: &str) -> AppResult<(i64, i64)> {
        let query = queries::fill(
            queries::GET_TERMS_VERSION_QUERY,
            &[("product_code", product_code)],
        );
        let result = self.query_service.execute_gql_query(&query)?;
        let version = result
            .get("termsAndConditionsForProduct")
            .and_then(|node| node.get("version"))
            .and_then(|value| value.as_str())
            .unwrap_or("1.0");
        let parts: Vec<&str> = version.split('.').collect();
        let major = parts.first().and_then(|value| value.parse::<i64>().ok()).unwrap_or(1);
        let minor = parts.get(1).and_then(|value| value.parse::<i64>().ok()).unwrap_or(0);
        Ok((major, minor))
    }

    /// `AccountManager.fetch_current_account_info`
    pub fn fetch_current_account_info(&mut self) -> AppResult<AccountInfo> {
        let settings = config::get();
        let query = queries::fill(queries::ACCOUNT_QUERY, &[("acc_number", &settings.acc_number)]);
        let result = self.query_service.execute_gql_query(&query)?;

        let agreements = result
            .get("account")
            .and_then(|account| account.get("electricityAgreements"))
            .and_then(|value| value.as_array());
        let agreements = match agreements {
            Some(value) => value.clone(),
            None => Vec::new(),
        };

        let mut import_agreement: Option<&Value> = None;
        for agreement in &agreements {
            let direction = agreement
                .get("meterPoint")
                .and_then(|point| point.get("direction"))
                .and_then(|value| value.as_str());
            if direction == Some("IMPORT") {
                import_agreement = Some(agreement);
                break;
            }
        }
        let import_agreement = match import_agreement {
            Some(value) => value,
            None => return Err(AppError::new("ERROR: No IMPORT meter point found in account data")),
        };

        let tariff_data = match import_agreement.get("tariff") {
            Some(value) if !value.is_null() => value,
            _ => {
                return Err(AppError::new(
                    "ERROR: No tariff information found for the IMPORT meter",
                ))
            }
        };
        let tariff_code = match tariff_data.get("tariffCode").and_then(|value| value.as_str()) {
            Some(value) if !value.is_empty() => value.to_string(),
            _ => return Err(AppError::new("ERROR: No tariff code found for the IMPORT tariff")),
        };
        let standing_charge_value = tariff_data
            .get("standingCharge")
            .ok_or_else(|| key_error("standingCharge"))?;
        if standing_charge_value.is_null() {
            return Err(AppError::new(
                "ERROR: No standing charge found for the IMPORT meter tariff",
            ));
        }
        let standing_charge = expect_float(standing_charge_value)?;

        self.region_code = tariff_code.chars().last().map(|ch| ch.to_string());
        let meter_point = import_agreement.get("meterPoint").cloned().unwrap_or(Value::Null);
        let mpan = meter_point
            .get("mpan")
            .and_then(|value| value.as_str())
            .map(|value| value.to_string());
        self.mpan = match mpan {
            Some(value) if !value.is_empty() => Some(value),
            _ => return Err(AppError::new("ERROR: No MPAN found for the IMPORT meter")),
        };

        let product_code = tariff_data
            .get("productCode")
            .and_then(|value| value.as_str())
            .map(|value| value.to_string());
        let product_code = match product_code {
            Some(value) if !value.is_empty() => value,
            _ => return Err(AppError::new("ERROR: No product code found for the IMPORT tariff")),
        };

        let matching = self
            .available_tariffs
            .iter()
            .find(|tariff| tariff.is_tariff(&tariff_code))
            .copied();
        let matching = match matching {
            Some(value) => value,
            None => {
                return Err(AppError::new(format!(
                    "ERROR: Found no supported tariff object for '{}' among available tariffs.",
                    tariff_code
                )))
            }
        };

        let consumption_data = self.fetch_consumption(&meter_point)?;

        let info = AccountInfo {
            current_tariff: matching,
            standing_charge,
            region_code: self.region_code.clone().unwrap_or_default(),
            consumption: consumption_data,
            mpan: self.mpan.clone().unwrap_or_default(),
            product_code: Some(product_code),
        };
        self.current_account_info = Some(info.clone());
        Ok(info)
    }

    /// `AccountManager._fetch_consumption`
    fn fetch_consumption(&mut self, meter_point: &Value) -> AppResult<Vec<Value>> {
        let settings = config::get();
        if settings.consumption_source == "homeassistant" {
            logi!(
                "octobot.account_manager",
                "account_manager._fetch_consumption",
                "Consumption source: Home Assistant"
            );
            return HomeAssistantClient::new(None, None, None).get_today_half_hourly_consumption();
        }

        logi!(
            "octobot.account_manager",
            "account_manager._fetch_consumption",
            "Consumption source: Octopus smartMeterTelemetry"
        );
        self.device_id = None;
        if let Some(meters) = meter_point.get("meters").and_then(|value| value.as_array()) {
            for meter in meters {
                if let Some(devices) = meter.get("smartDevices").and_then(|value| value.as_array()) {
                    for device in devices {
                        if let Some(id) = device.get("deviceId").and_then(|value| value.as_str()) {
                            self.device_id = Some(id.to_string());
                            break;
                        }
                    }
                }
                if self.device_id.is_some() {
                    break;
                }
            }
        }

        let device_id = match &self.device_id {
            Some(value) => value.clone(),
            None => {
                return Err(AppError::new(
                    "ERROR: No device ID found for the IMPORT meter. Set \
                     CONSUMPTION_SOURCE=homeassistant to read usage from Home Assistant instead.",
                ))
            }
        };

        let today = clock::today_iso();
        let query = queries::fill(
            queries::CONSUMPTION_QUERY,
            &[
                ("device_id", &device_id),
                ("start_date", &format!("{}T00:00:00Z", today)),
                ("end_date", &format!("{}T23:59:59Z", today)),
            ],
        );
        let result = self.query_service.execute_gql_query(&query)?;
        Ok(result
            .get("smartMeterTelemetry")
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default())
    }

    /// `AccountManager.initiate_tariff_switch`
    pub fn initiate_tariff_switch(&mut self, target_product_code: &str) -> AppResult<Option<String>> {
        if self.mpan.is_none() {
            logi!(
                "octobot.account_manager",
                "account_manager.initiate_tariff_switch",
                "MPAN not readily available, fetching account details first..."
            );
            self.fetch_current_account_info()?;
            if self.mpan.is_none() {
                return Err(AppError::new(
                    "ERROR: MPAN could not be determined. Cannot switch tariff.",
                ));
            }
        }
        let settings = config::get();
        let query = queries::fill(
            queries::SWITCH_QUERY,
            &[
                ("account_number", &settings.acc_number),
                ("mpan", self.mpan.as_deref().unwrap_or("")),
                ("product_code", target_product_code),
                ("change_date", &clock::today_iso()),
            ],
        );
        let result = self.query_service.execute_gql_query(&query)?;
        Ok(result
            .get("startOnboardingProcess")
            .and_then(|node| node.get("productEnrolment"))
            .and_then(|node| node.get("id"))
            .and_then(|value| value.as_str())
            .map(|value| value.to_string()))
    }

    /// `AccountManager.accept_new_agreement`
    pub fn accept_new_agreement(&self, product_code: &str, enrolment_id: &str) -> AppResult<String> {
        let (major, minor) = self.get_agreement_terms_version(product_code)?;
        let settings = config::get();
        let query = queries::fill(
            queries::ACCEPT_TERMS_QUERY,
            &[
                ("account_number", &settings.acc_number),
                ("enrolment_id", enrolment_id),
                ("version_major", &major.to_string()),
                ("version_minor", &minor.to_string()),
            ],
        );
        let result = self.query_service.execute_gql_query(&query)?;
        Ok(result
            .get("acceptTermsAndConditions")
            .and_then(|node| node.get("acceptedVersion"))
            .and_then(|value| value.as_str())
            .unwrap_or("unknown version")
            .to_string())
    }

    /// `AccountManager.verify_new_agreement_status`
    pub fn verify_new_agreement_status(&self) -> AppResult<bool> {
        let settings = config::get();
        let query = queries::fill(queries::ACCOUNT_QUERY, &[("acc_number", &settings.acc_number)]);
        let result = self.query_service.execute_gql_query(&query)?;

        let today = clock::today();
        if let Some(agreements) = result
            .get("account")
            .and_then(|account| account.get("electricityAgreements"))
            .and_then(|value| value.as_array())
        {
            for agreement in agreements {
                let valid_from = match agreement.get("validFrom").and_then(|value| value.as_str()) {
                    Some(value) => value,
                    None => continue,
                };
                match clock::parse_iso(valid_from) {
                    Some(parsed) => {
                        if parsed.date_naive() == today {
                            return Ok(true);
                        }
                    }
                    None => {
                        logw!(
                            "octobot.account_manager",
                            "account_manager.verify_new_agreement_status",
                            "Could not parse agreement 'validFrom' date: {}",
                            valid_from
                        );
                    }
                }
            }
        }
        Ok(false)
    }
}
