//! `src/account_info.py`

use crate::tariff::Tariff;

#[derive(Clone)]
pub struct AccountInfo {
    pub current_tariff: &'static Tariff,
    pub standing_charge: f64,
    pub region_code: String,
    pub consumption: Vec<serde_json::Value>,
    pub mpan: String,
    /// The Octopus product code for the current tariff.
    pub product_code: Option<String>,
}
