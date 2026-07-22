//! `FilamentProfile` (filament and resin materials). See `SPEC.md` §2.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MaterialKind {
    Filament,
    Resin,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilamentProfile {
    pub name: String,
    pub kind: MaterialKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price_per_kg: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price_per_liter: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub density_g_per_cm3: Option<f64>,
    pub efficiency_factor: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vendor: Option<String>,
}
