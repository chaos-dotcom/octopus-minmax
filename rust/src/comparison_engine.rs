//! `src/comparison_engine.py`

use serde_json::Value;

use crate::account_info::AccountInfo;
use crate::config;
use crate::errors::{key_error, AppError, AppResult};
use crate::pyrepr::{fixed, repr_float, round_trip_4};
use crate::query_service::{expect_float, QueryService};
use crate::tariff::Tariff;
use crate::{logd, logw};

#[derive(Clone, Debug)]
pub struct CostBreakdown {
    pub consumption_cost: f64,
    pub standing_charge: f64,
    pub total_cost: f64,
    pub total_kwh: f64,
    /// `sum([])` is the *int* 0 in Python, which reprs as `0` rather than `0.0`.
    pub consumption_cost_is_int: bool,
}

impl CostBreakdown {
    pub fn total_cost_pounds(&self) -> f64 {
        self.total_cost / 100.0
    }
    pub fn consumption_cost_pounds(&self) -> f64 {
        self.consumption_cost / 100.0
    }
    pub fn standing_charge_pounds(&self) -> f64 {
        self.standing_charge / 100.0
    }
    /// `CostBreakdown(consumption_cost=..., standing_charge=..., total_cost=..., total_kwh=...)`
    pub fn python_repr(&self) -> String {
        format!(
            "CostBreakdown(consumption_cost={}, standing_charge={}, total_cost={}, total_kwh={})",
            if self.consumption_cost_is_int {
                (self.consumption_cost as i64).to_string()
            } else {
                repr_float(self.consumption_cost)
            },
            repr_float(self.standing_charge),
            repr_float(self.total_cost),
            repr_float(self.total_kwh)
        )
    }
}

#[derive(Clone, Debug)]
pub struct TariffComparison {
    pub tariff: &'static Tariff,
    pub cost_breakdown: Option<CostBreakdown>,
    pub error: Option<String>,
}

impl TariffComparison {
    pub fn new(tariff: &'static Tariff, cost_breakdown: Option<CostBreakdown>) -> TariffComparison {
        TariffComparison { tariff, cost_breakdown, error: None }
    }

    pub fn is_valid(&self) -> bool {
        self.error.is_none() && self.cost_breakdown.is_some()
    }

    pub fn total_cost(&self) -> Option<f64> {
        self.cost_breakdown.as_ref().map(|breakdown| breakdown.total_cost)
    }

    /// The dataclass repr the Python engine logs.  The tariff part is the default
    /// object repr, so it carries a real address (the Python original does too).
    pub fn python_repr(&self) -> String {
        format!(
            "TariffComparison(tariff=<tariff.Tariff object at 0x{:x}>, cost_breakdown={}, error={})",
            self.tariff.python_address(),
            match &self.cost_breakdown {
                Some(breakdown) => breakdown.python_repr(),
                None => "None".to_string(),
            },
            match &self.error {
                Some(message) => crate::pyrepr::repr_str(message),
                None => "None".to_string(),
            }
        )
    }
}

pub struct ComparisonResult {
    pub current_tariff_comparison: TariffComparison,
    pub alternative_comparisons: Vec<TariffComparison>,
    pub cheapest_tariff: Option<&'static Tariff>,
    pub potential_savings: f64,
}

impl ComparisonResult {
    /// `ComparisonResult.should_switch`
    pub fn should_switch(&self) -> bool {
        let cheapest_name = self
            .cheapest_tariff
            .map(|tariff| tariff.display_name)
            .unwrap_or("None");
        let threshold = config::get().switch_threshold;
        logd!(
            "octobot.comparison_engine",
            "comparison_engine.should_switch",
            "cheapest_tariff: {}, potential_savings: {}, SWITCH_THRESHOLD: {}",
            cheapest_name,
            repr_float(self.potential_savings),
            threshold
        );
        if self.cheapest_tariff.is_none() {
            return false;
        }
        let cheapest = self.cheapest_tariff.unwrap();
        if cheapest == self.current_tariff_comparison.tariff {
            return false;
        }
        self.potential_savings > threshold as f64
    }

    pub fn all_comparisons(&self) -> Vec<&TariffComparison> {
        let mut all: Vec<&TariffComparison> = vec![&self.current_tariff_comparison];
        all.extend(self.alternative_comparisons.iter());
        all
    }
}

pub struct ComparisonEngine {
    query_service: std::sync::Arc<QueryService>,
}

impl ComparisonEngine {
    pub fn new(query_service: std::sync::Arc<QueryService>) -> ComparisonEngine {
        logd!("octobot.comparison_engine", "comparison_engine.__init__", "Initialising ComparisonEngine");
        ComparisonEngine { query_service }
    }

    pub fn compare_tariffs(
        &self,
        account_info: &AccountInfo,
        available_tariffs: &[&'static Tariff],
    ) -> AppResult<ComparisonResult> {
        let current_costs = self.calculate_current_cost(account_info)?;
        let current_comparison =
            TariffComparison::new(account_info.current_tariff, Some(current_costs));

        let mut alternative_comparisons = Vec::new();
        for tariff in available_tariffs {
            if *tariff == account_info.current_tariff {
                continue;
            }
            alternative_comparisons.push(self.compare_tariff(tariff, account_info)?);
        }

        let rendered: Vec<String> = alternative_comparisons
            .iter()
            .map(TariffComparison::python_repr)
            .collect();
        logd!(
            "octobot.comparison_engine",
            "comparison_engine.compare_tariffs",
            "Tariff comparison results - [{}]",
            rendered.join(", ")
        );

        let (cheapest_tariff, potential_savings) =
            self.find_best_option(&current_comparison, &alternative_comparisons);

        Ok(ComparisonResult {
            current_tariff_comparison: current_comparison,
            alternative_comparisons,
            cheapest_tariff,
            potential_savings,
        })
    }

    fn find_best_option(
        &self,
        current: &TariffComparison,
        alternatives: &[TariffComparison],
    ) -> (Option<&'static Tariff>, f64) {
        let mut valid: Vec<&TariffComparison> = Vec::new();
        if current.is_valid() {
            valid.push(current);
        }
        for comparison in alternatives {
            if comparison.is_valid() && comparison.tariff.switchable {
                valid.push(comparison);
            }
        }
        if valid.is_empty() {
            return (None, 0.0);
        }
        // `min()` keeps the first minimum.
        let mut cheapest = valid[0];
        for candidate in &valid[1..] {
            if candidate.total_cost().unwrap_or(f64::INFINITY) < cheapest.total_cost().unwrap_or(f64::INFINITY) {
                cheapest = candidate;
            }
        }
        let current_cost = if current.is_valid() {
            current.total_cost().unwrap_or(f64::INFINITY)
        } else {
            f64::INFINITY
        };
        let cheapest_cost = if cheapest.is_valid() {
            cheapest.total_cost().unwrap_or(f64::INFINITY)
        } else {
            f64::INFINITY
        };
        (Some(cheapest.tariff), current_cost - cheapest_cost)
    }

    fn compare_tariff(
        &self,
        tariff: &'static Tariff,
        account_info: &AccountInfo,
    ) -> AppResult<TariffComparison> {
        let (standing_charge, unit_rates, product_code) =
            self.get_potential_tariff_rates(tariff, &account_info.region_code)?;
        tariff.set_product_code(Some(product_code));

        let period_costs = self.calculate_potential_costs(&account_info.consumption, &unit_rates)?;
        // Rust's float `Sum` starts from `-0.0`; Python's `sum([])` is the int 0.
        let consumption_cost_is_int = period_costs.is_empty();
        let consumption_cost: f64 = if consumption_cost_is_int {
            0.0
        } else {
            period_costs.iter().map(|period| period.calculated_cost).sum::<f64>()
        };
        let total_cost = consumption_cost + standing_charge;

        let mut total_wh = 0.0f64;
        for entry in &account_info.consumption {
            total_wh += expect_float(entry.get("consumptionDelta").unwrap_or(&Value::from(0)))?;
        }
        let total_kwh = total_wh / 1000.0;

        Ok(TariffComparison::new(
            tariff,
            Some(CostBreakdown {
                consumption_cost,
                standing_charge,
                total_cost,
                total_kwh,
                consumption_cost_is_int,
            }),
        ))
    }

    fn calculate_current_cost(&self, account_info: &AccountInfo) -> AppResult<CostBreakdown> {
        let mut total_wh = 0.0f64;
        for consumption in &account_info.consumption {
            let delta = consumption.get("consumptionDelta").ok_or_else(|| key_error("consumptionDelta"))?;
            total_wh += expect_float(delta)?;
        }
        let total_kwh = total_wh / 1000.0;
        if total_kwh == 0.0 {
            return Err(AppError::new(
                "No consumption data found for today. Check your consumption source.",
            ));
        }

        let cost_deltas: Vec<Option<&Value>> = account_info
            .consumption
            .iter()
            .map(|entry| entry.get("costDeltaWithTax"))
            .collect();
        let any_present = cost_deltas.iter().any(|value| match value {
            Some(Value::Null) | None => false,
            _ => true,
        });

        let consumption_cost = if any_present {
            let mut total = 0.0f64;
            for delta in cost_deltas {
                total += match delta {
                    None | Some(Value::Null) => 0.0,
                    Some(value) => expect_float(value)?,
                };
            }
            total
        } else {
            let product_code = account_info.product_code.clone().unwrap_or_default();
            let (_standing, unit_rates) = self.get_product_rates(&product_code, &account_info.region_code)?;
            let period_costs = self.calculate_potential_costs(&account_info.consumption, &unit_rates)?;
            period_costs.iter().map(|period| period.calculated_cost).sum::<f64>()
        };

        Ok(CostBreakdown {
            consumption_cost,
            standing_charge: account_info.standing_charge,
            total_cost: consumption_cost + account_info.standing_charge,
            total_kwh,
            consumption_cost_is_int: false,
        })
    }

    fn calculate_potential_costs(
        &self,
        consumption_data: &[Value],
        rate_data: &[Value],
    ) -> AppResult<Vec<PeriodCost>> {
        let mut period_costs = Vec::new();
        for consumption in consumption_data {
            let read_at_raw = consumption.get("readAt").ok_or_else(|| key_error("readAt"))?;
            let read_time = match read_at_raw.as_str() {
                Some(text) => text.replace("+00:00", "Z"),
                None => String::new(),
            };
            let mut matching: Option<&Value> = None;
            for rate in rate_data {
                let valid_from = rate.get("valid_from").ok_or_else(|| key_error("valid_from"))?;
                let valid_from = valid_from.as_str().unwrap_or("");
                let valid_to = match rate.get("valid_to") {
                    Some(Value::Null) | None => "9999-12-31T23:59:59Z".to_string(),
                    Some(value) => match value.as_str() {
                        Some(text) if !text.is_empty() => text.to_string(),
                        _ => "9999-12-31T23:59:59Z".to_string(),
                    },
                };
                let payment_method = rate.get("payment_method").ok_or_else(|| key_error("payment_method"))?;
                let payment_ok = match payment_method {
                    Value::Null => true,
                    Value::String(text) => text == "DIRECT_DEBIT",
                    _ => false,
                };
                if valid_from <= read_time.as_str()
                    && read_time.as_str() <= valid_to.as_str()
                    && payment_ok
                {
                    matching = Some(rate);
                    break;
                }
            }

            let matching_rate = match matching {
                Some(rate) => rate,
                None => {
                    logw!(
                        "octobot.comparison_engine",
                        "comparison_engine._calculate_potential_costs",
                        "No rate covers {}; skipping that period",
                        read_time
                    );
                    continue;
                }
            };

            let delta = consumption.get("consumptionDelta").ok_or_else(|| key_error("consumptionDelta"))?;
            let consumption_kwh = expect_float(delta)? / 1000.0;
            let rate_value = matching_rate
                .get("value_inc_vat")
                .ok_or_else(|| key_error("value_inc_vat"))?;
            let rate = expect_float(rate_value)?;
            let cost = round_trip_4(consumption_kwh * rate);

            period_costs.push(PeriodCost {
                period_end: read_time,
                consumption_kwh,
                rate,
                calculated_cost: cost,
            });
        }
        Ok(period_costs)
    }

    fn find_product_code(&self, display_name: &str) -> AppResult<String> {
        let all_products = self
            .query_service
            .execute_rest_query(&format!(
                "{}/products/?brand=OCTOPUS_ENERGY&is_business=false",
                self.query_service.base_url()
            ))?;
        let results = match all_products.get("results").and_then(|items| items.as_array()) {
            Some(items) => items,
            None => return Err(key_error("results")),
        };
        let mut product_code: Option<String> = None;
        for product in results {
            let name = product.get("display_name").ok_or_else(|| key_error("display_name"))?;
            let direction = product.get("direction").ok_or_else(|| key_error("direction"))?;
            if name.as_str() == Some(display_name) && direction.as_str() == Some("IMPORT") {
                product_code = product.get("code").and_then(|code| code.as_str()).map(|code| code.to_string());
                break;
            }
        }
        let product_code = match product_code {
            Some(code) => code,
            None => {
                return Err(AppError::new(format!("No matching tariff found for {}", display_name)))
            }
        };
        if product_code.is_empty() {
            return Err(AppError::new(format!("No product code found for {}", display_name)));
        }
        Ok(product_code)
    }

    fn get_product_rates(&self, product_code: &str, region_code: &str) -> AppResult<(f64, Vec<Value>)> {
        if product_code.is_empty() {
            return Err(AppError::new("No product code supplied to fetch rates for"));
        }
        let tariff_details = self
            .query_service
            .execute_rest_query(&format!("{}/products/{}/", self.query_service.base_url(), product_code))?;

        let region_code_key = format!("_{}", region_code);
        let filtered_region = tariff_details
            .get("single_register_electricity_tariffs")
            .and_then(|value| value.get(&region_code_key));
        let filtered_region = match filtered_region {
            Some(value) => value,
            None => return Err(AppError::new(format!("Region code not found: {}", region_code_key))),
        };

        let direct_debit = filtered_region.get("direct_debit_monthly");
        let region_tariffs = match direct_debit {
            Some(value) if !value.is_null() => value,
            _ => match filtered_region.get("varying") {
                Some(value) => value,
                None => return Err(AppError::new(format!("Region code not found: {}", region_code_key))),
            },
        };

        let standing_charge_inc_vat = match region_tariffs.get("standing_charge_inc_vat") {
            Some(Value::Null) | None => {
                return Err(AppError::new(format!(
                    "Standing charge including VAT not found for region {}.",
                    region_code_key
                )))
            }
            Some(value) => expect_float(value)?,
        };

        let empty = Vec::new();
        let region_links = region_tariffs
            .get("links")
            .and_then(|value| value.as_array())
            .unwrap_or(&empty);
        let mut unit_rates_link: Option<String> = None;
        for item in region_links {
            let rel = item.get("rel").and_then(|value| value.as_str()).unwrap_or("");
            if rel.to_lowercase() == "standard_unit_rates" {
                unit_rates_link = item.get("href").and_then(|value| value.as_str()).map(|value| value.to_string());
                break;
            }
        }
        let unit_rates_link = match unit_rates_link {
            Some(link) => link,
            None => {
                return Err(AppError::new(format!(
                    "Standard unit rates link not found for region: {}",
                    region_code_key
                )))
            }
        };

        let today = crate::clock::today();
        let period_from = today - chrono::Duration::days(1);
        let period_to = today + chrono::Duration::days(1);
        let url = format!(
            "{}?period_from={}T00:00:00Z&period_to={}T23:59:59Z",
            unit_rates_link,
            period_from.format("%Y-%m-%d"),
            period_to.format("%Y-%m-%d")
        );
        let unit_rates = self.query_service.execute_rest_query(&url)?;
        let results = unit_rates
            .get("results")
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default();
        Ok((standing_charge_inc_vat, results))
    }

    fn get_potential_tariff_rates(
        &self,
        tariff: &Tariff,
        region_code: &str,
    ) -> AppResult<(f64, Vec<Value>, String)> {
        let product_code = self.find_product_code(tariff.api_display_name)?;
        let (standing_charge, unit_rates) = self.get_product_rates(&product_code, region_code)?;
        Ok((standing_charge, unit_rates, product_code))
    }
}

#[derive(Clone, Debug)]
pub struct PeriodCost {
    pub period_end: String,
    pub consumption_kwh: f64,
    pub rate: f64,
    pub calculated_cost: f64,
}

#[allow(dead_code)]
pub fn pounds(value: f64) -> String {
    fixed(value, 2)
}
