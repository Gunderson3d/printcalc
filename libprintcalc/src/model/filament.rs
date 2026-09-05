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

/// `PRICING.md`'s default for `K` — spool price, per kg of filament or
/// per liter of resin.
pub const DEFAULT_PRICE: f64 = 20.0;
/// `PRICING.md`'s default for `m` — the material markup covering purge,
/// waste and failures.
pub const DEFAULT_EFFICIENCY_FACTOR: f64 = 1.5;

fn default_price() -> f64 {
    DEFAULT_PRICE
}
fn default_efficiency_factor() -> f64 {
    DEFAULT_EFFICIENCY_FACTOR
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
    /// `PRICING.md`'s `K`.
    #[serde(default = "default_price")]
    pub price: f64,
    /// Waste/purge multiplier applied to raw usage before pricing (e.g.
    /// `1.1` = price for 10% more than the slicer reported was actually
    /// used). `PRICING.md`'s `m`.
    #[serde(default = "default_efficiency_factor")]
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `PRICING.md` gives `K` and `m` defaults; a profile naming only the
    /// material still prices.
    #[test]
    fn omitted_price_and_factor_take_the_pricing_md_defaults() {
        let profile: MaterialProfile = toml::from_str(
            r#"
name = "sparse"
kind = "filament"
"#,
        )
        .unwrap();
        assert_eq!(profile.price, 20.0);
        assert_eq!(profile.efficiency_factor, 1.5);
    }

    /// The same typo trap the printer profile has: a misspelled `price`
    /// now costs at the default instead of failing, so it has to be
    /// reported.
    #[test]
    fn a_misspelled_field_is_reported_as_unknown() {
        let (profile, unknown) = crate::parse_toml::<MaterialProfile>(
            "name = \"typo\"\nkind = \"filament\"\npryce = 31.0\n",
        )
        .unwrap();
        assert_eq!(unknown, ["pryce"]);
        assert_eq!(profile.price, DEFAULT_PRICE);
    }
}
