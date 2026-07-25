//! `PrinterProfile` and its wear/depreciation model. See `SPEC.md` §2, §3.

use serde::{Deserialize, Serialize};

/// Which physical process a printer uses — determines which sliced
/// formats (`SPEC.md` §4) and filament kinds ([`super::filament::MaterialKind`])
/// are compatible with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PrinterKind {
    /// Fused deposition modeling (filament-based).
    Fdm,
    /// Resin (SLA/MSLA).
    Resin,
}

/// A reusable printer definition, saved under `~/.config/printcalc/printers/`
/// and referenced from jobs by name (or embedded directly). See `SPEC.md` §5.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrinterProfile {
    /// Also the filename stem it's saved under.
    pub name: String,
    /// FDM or resin.
    pub kind: PrinterKind,
    /// Depreciation and running-cost inputs.
    pub wear: WearProfile,
}

/// Depreciation and running-cost inputs for [`PrinterProfile`], consumed by
/// [`super::cost::calculate_cost`] (`SPEC.md` §3). Electricity price isn't
/// here — it's a per-job or global-config value, since it depends on where
/// you pay your bill from, not which printer ran the job.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WearProfile {
    /// What the printer itself cost.
    pub purchase_price: f64,
    /// Anything else spent getting it running (upgrades, shipping, ...).
    pub additional_upfront_cost: f64,
    /// Expected working lifetime, in years, before replacement.
    pub estimated_life_years: f64,
    /// Fraction of the year (`0.0`-`1.0`) the printer is actually printing.
    pub estimated_uptime_percent: f64,
    /// Expected yearly maintenance spend (nozzles, belts, ...).
    pub annual_maintenance_cost: f64,
    /// The printer's own power draw while printing.
    pub power_consumption_watts: f64,
    /// Multiplier applied to the computed wear/electricity rate — a safety
    /// margin over the raw depreciation math (`SPEC.md` §3).
    pub cost_buffer_factor: f64,
}
