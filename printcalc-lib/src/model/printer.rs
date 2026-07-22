//! `PrinterProfile` and its wear/depreciation model. See `SPEC.md` §2, §3.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PrinterKind {
    Fdm,
    Resin,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrinterProfile {
    pub name: String,
    pub kind: PrinterKind,
    pub wear: WearProfile,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WearProfile {
    pub purchase_price: f64,
    pub additional_upfront_cost: f64,
    pub estimated_life_years: f64,
    pub estimated_uptime_percent: f64,
    pub annual_maintenance_cost: f64,
    pub power_consumption_watts: f64,
    pub cost_buffer_factor: f64,
}
