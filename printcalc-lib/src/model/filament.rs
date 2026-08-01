//! `MaterialProfile` (filament and resin materials).

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
/// [`super::job::Job::material`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterialProfile {
    /// Also the filename stem it's saved under.
    pub name: String,
    /// Filament or resin.
    pub kind: MaterialKind,
    /// Price per unit of material — per kilogram for
    /// [`MaterialKind::Filament`], per liter for [`MaterialKind::Resin`].
    pub price: f64,
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

impl MaterialProfile {
    /// Serializes this profile to the TOML the profile library saves and
    /// [`crate::parse_material`] reads back.
    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string(self)
    }
}
