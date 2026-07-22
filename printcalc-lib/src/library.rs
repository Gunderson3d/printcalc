//! Profile library load/save (`~/.config/printcalc/{printers,filaments}/*.toml`)
//! and global config (`~/.config/printcalc/config.toml`). See `SPEC.md` §6.
//!
//! Filesystem-only — not available to `printcalc-wasm`, which has no
//! filesystem and instead receives embedded profiles directly.
//!
//! Load/save against `~/.config/printcalc/` lands with the profile-CRUD
//! `ops` functions (`SPEC.md` §9 milestone 3); for now this just holds the
//! `GlobalConfig` shape so [`crate::model::cost`] has something to take
//! defaults from.

use serde::{Deserialize, Serialize};

/// `~/.config/printcalc/config.toml`. See `SPEC.md` §5.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlobalConfig {
    pub electricity_cost_per_kwh: f64,
    pub labor_rate_per_hour: f64,
    pub markup_percentages: Vec<f64>,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        GlobalConfig {
            electricity_cost_per_kwh: 0.14,
            labor_rate_per_hour: 15.0,
            markup_percentages: vec![50.0, 60.0, 70.0],
        }
    }
}
