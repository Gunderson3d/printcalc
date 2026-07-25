//! `FilamentProfile` (filament and resin materials).

use serde::{Deserialize, Serialize};

/// Whether this is a spooled filament (priced/measured by weight) or a
/// resin (typically priced/measured by volume). Must match the printer
/// ([`super::printer::PrinterKind`]) and the sliced file's format it's
/// costed against — [`super::cost::calculate_cost`] rejects a mismatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MaterialKind {
    /// Spooled filament (FDM).
    Filament,
    /// Liquid resin (SLA/MSLA).
    Resin,
}

/// A reusable material definition, saved under `~/.config/printcalc/filaments/`
/// and referenced from jobs by name (or embedded directly), keyed by slot in
/// [`super::job::Job::filament`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilamentProfile {
    /// Also the filename stem it's saved under.
    pub name: String,
    /// Filament or resin.
    pub kind: MaterialKind,
    /// Price per kilogram — the common case for filament. At least one of
    /// `price_per_kg`/`price_per_liter` must be set (or reachable via
    /// `density_g_per_cm3` conversion from the other) for the profile to
    /// be costable against a given usage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price_per_kg: Option<f64>,
    /// Price per liter — the common case for resin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price_per_liter: Option<f64>,
    /// Grams per cm³ (= grams per ml). Lets [`super::cost::calculate_cost`]
    /// convert between grams and ml when a usage's unit doesn't match
    /// whichever price field is set — e.g. a resin priced per kg costed
    /// against an ml-reported usage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub density_g_per_cm3: Option<f64>,
    /// Waste/purge multiplier applied to raw usage before pricing (e.g.
    /// `1.1` = price for 10% more than the slicer reported was actually
    /// used).
    pub efficiency_factor: f64,
    /// Free-form display color, e.g. `"black"` or a hex code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Free-form manufacturer name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vendor: Option<String>,
}
