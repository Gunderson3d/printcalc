//! `MaterialProfile`: what a material costs and how much of it you're
//! charged for.
//!
//! One type covers both filament and resin. [`MaterialKind`] decides how
//! `price` is read — per kilogram against grams, or per litre against
//! millilitres — and costing rejects a profile whose kind doesn't match
//! the sliced file's process, so no unit conversion is ever needed.

use serde::{Deserialize, Serialize};

/// Whether this is a spooled filament (priced/measured by weight) or a
/// resin (typically priced/measured by volume). Must match the printer
/// ([`super::printer::PrinterKind`]) and the sliced file's format it's
/// costed against — [`super::cost::calculate_cost`] rejects a mismatch.
///
/// `#[non_exhaustive]`: other processes (SLS powder, ...) may be added.
/// Match with a wildcard arm, or use [`MaterialKind::price_unit`] rather
/// than matching at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum MaterialKind {
    /// Spooled filament (FDM).
    Filament,
    /// Liquid resin (SLA/MSLA).
    Resin,
}

impl MaterialKind {
    /// What [`MaterialProfile::price`] is per: `"kg"` for filament,
    /// `"L"` for resin.
    pub fn price_unit(self) -> &'static str {
        match self {
            MaterialKind::Filament => "kg",
            MaterialKind::Resin => "L",
        }
    }

    /// What usage is measured in: `"g"` for filament, `"ml"` for resin.
    pub fn usage_unit(self) -> &'static str {
        match self {
            MaterialKind::Filament => "g",
            MaterialKind::Resin => "ml",
        }
    }
}

/// Default spool price when a profile doesn't state one — per kg of
/// filament, per liter of resin.
pub const DEFAULT_PRICE: f64 = 20.0;
/// Default waste allowance when a profile doesn't state one, covering
/// purge, skirts and failed prints.
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
///
/// `#[non_exhaustive]`: build one with [`MaterialProfile::new`] and the
/// `with_*` setters, or parse one from TOML. Fields stay public to read
/// and assign.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct MaterialProfile {
    /// Also the filename stem it's saved under.
    pub name: String,
    /// Filament or resin.
    pub kind: MaterialKind,
    /// Price per unit of material — per kilogram for
    /// [`MaterialKind::Filament`], per liter for [`MaterialKind::Resin`].
    #[serde(default = "default_price")]
    pub price: f64,
    /// Waste/purge multiplier applied to raw usage before pricing (e.g.
    /// `1.5` = price for 50% more than the slicer reported was actually
    /// used), covering purge, skirts and failed prints.
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
    /// A profile at the default price and efficiency factor, with no
    /// color or vendor. Chain the `with_*` setters to fill in the rest.
    pub fn new(name: impl Into<String>, kind: MaterialKind) -> Self {
        MaterialProfile {
            name: name.into(),
            kind,
            price: DEFAULT_PRICE,
            efficiency_factor: DEFAULT_EFFICIENCY_FACTOR,
            color: None,
            vendor: None,
        }
    }

    /// Sets the price per kg (filament) or per liter (resin).
    pub fn with_price(mut self, price: f64) -> Self {
        self.price = price;
        self
    }

    /// Sets the waste/purge multiplier.
    pub fn with_efficiency_factor(mut self, factor: f64) -> Self {
        self.efficiency_factor = factor;
        self
    }

    /// Sets the display color.
    pub fn with_color(mut self, color: impl Into<String>) -> Self {
        self.color = Some(color.into());
        self
    }

    /// Sets the manufacturer name.
    pub fn with_vendor(mut self, vendor: impl Into<String>) -> Self {
        self.vendor = Some(vendor.into());
        self
    }

    /// Serializes this profile to the TOML the profile library saves and
    /// [`crate::parse_material`] reads back.
    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Price and waste allowance both default, so a profile naming only
    /// the material still prices.
    #[test]
    fn omitted_price_and_factor_take_the_documented_defaults() {
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
        assert!((profile.price - DEFAULT_PRICE).abs() < f64::EPSILON);
    }
}
