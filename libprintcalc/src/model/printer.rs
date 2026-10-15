//! `PrinterProfile`: what an hour on a machine costs.
//!
//! [`WearProfile`] holds the depreciation inputs — what the machine cost,
//! how long it should last, how much of that time it actually prints —
//! which [`super::cost`] turns into an hourly rate. A shop that already
//! knows its hourly rate can state it directly with
//! [`WearProfile::print_time_rate_override`] and skip the derivation.
//!
//! Electricity price is deliberately not here: it depends on where you
//! pay your bill, not on which printer ran the job, so it lives in the
//! job or the global config.

use serde::{Deserialize, Serialize};

/// Which physical process a printer uses — determines which sliced
/// formats and filament kinds ([`super::filament::MaterialKind`])
/// are compatible with it.
///
/// `#[non_exhaustive]`: other processes may be added. Match with a
/// wildcard arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum PrinterKind {
    /// Fused deposition modeling (filament-based).
    Fdm,
    /// Resin (SLA/MSLA).
    Resin,
}

/// A reusable printer definition, saved under `~/.config/printcalc/printers/`
/// and referenced from jobs by name (or embedded directly).
///
/// `#[non_exhaustive]`: build one with [`PrinterProfile::new`] or parse
/// one from TOML. Fields stay public to read and assign.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[non_exhaustive]
pub struct PrinterProfile {
    /// Also the filename stem it's saved under.
    pub name: String,
    /// FDM or resin.
    pub kind: PrinterKind,
    /// Depreciation and running-cost inputs — flattened, so its keys sit
    /// at the profile's top level in TOML (no `[wear]` table).
    #[serde(flatten)]
    pub wear: WearProfile,
}

/// The flat shape a printer profile has on disk. Deserializing through
/// this instead of `#[serde(flatten)]` is what lets
/// [`crate::parse_toml`] see an unknown key: a flattened field swallows
/// every key it doesn't recognize into its own buffered map, where
/// nothing outside can notice them. Serializing still flattens, so the
/// file format is unchanged.
///
/// Every wear field is optional here and falls back to
/// [`WearProfile::default`], keeping the defaults stated in exactly one
/// place.
#[derive(Deserialize)]
struct PrinterWire {
    name: String,
    kind: PrinterKind,
    #[serde(default)]
    purchase_price: Option<f64>,
    #[serde(default)]
    additional_upfront_cost: Option<f64>,
    #[serde(default)]
    estimated_life_years: Option<f64>,
    #[serde(default)]
    estimated_uptime_percent: Option<f64>,
    #[serde(default)]
    annual_maintenance_cost: Option<f64>,
    #[serde(default)]
    power_consumption_watts: Option<f64>,
    #[serde(default)]
    cost_buffer_factor: Option<f64>,
    #[serde(default)]
    print_time_rate_override: Option<f64>,
}

impl<'de> Deserialize<'de> for PrinterProfile {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = PrinterWire::deserialize(deserializer)?;
        let d = WearProfile::default();
        Ok(PrinterProfile {
            name: wire.name,
            kind: wire.kind,
            wear: WearProfile {
                purchase_price: wire.purchase_price.unwrap_or(d.purchase_price),
                additional_upfront_cost: wire
                    .additional_upfront_cost
                    .unwrap_or(d.additional_upfront_cost),
                estimated_life_years: wire.estimated_life_years.unwrap_or(d.estimated_life_years),
                estimated_uptime_percent: wire
                    .estimated_uptime_percent
                    .unwrap_or(d.estimated_uptime_percent),
                annual_maintenance_cost: wire
                    .annual_maintenance_cost
                    .unwrap_or(d.annual_maintenance_cost),
                power_consumption_watts: wire
                    .power_consumption_watts
                    .unwrap_or(d.power_consumption_watts),
                cost_buffer_factor: wire.cost_buffer_factor.unwrap_or(d.cost_buffer_factor),
                print_time_rate_override: wire.print_time_rate_override,
            },
        })
    }
}

/// Default purchase price when a profile doesn't state one.
pub const DEFAULT_PURCHASE_PRICE: f64 = 800.0;
/// Default additional upfront cost when a profile doesn't state one.
pub const DEFAULT_ADDITIONAL_UPFRONT_COST: f64 = 0.0;
/// Default annual maintenance spend when a profile doesn't state one.
pub const DEFAULT_ANNUAL_MAINTENANCE_COST: f64 = 80.0;
/// Default depreciation horizon, in years.
pub const DEFAULT_LIFE_YEARS: f64 = 5.0;
/// Default share of the year spent printing, `0.0`-`1.0`.
pub const DEFAULT_UPTIME_PERCENT: f64 = 0.50;
/// Default power draw while printing, in watts.
pub const DEFAULT_POWER_CONSUMPTION_WATTS: f64 = 160.0;
/// Default safety margin over the computed hourly rate.
pub const DEFAULT_COST_BUFFER_FACTOR: f64 = 1.3;

/// Depreciation and running-cost inputs for [`PrinterProfile`], consumed by
/// [`super::cost::calculate_cost`]. Electricity price isn't
/// here — it's a per-job or global-config value, since it depends on where
/// you pay your bill from, not which printer ran the job.
///
/// `#[non_exhaustive]`: start from [`WearProfile::new`] (the documented
/// defaults) and chain the `with_*` setters for what you know. Fields
/// stay public to read and assign.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[non_exhaustive]
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
    /// margin over the raw depreciation math.
    pub cost_buffer_factor: f64,
    /// A flat cost per printing hour that replaces the whole computed
    /// machine rate — depreciation, electricity and `cost_buffer_factor`
    /// alike — when set. For shops that already know what an hour on the
    /// machine costs them and would rather state it than derive it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub print_time_rate_override: Option<f64>,
}

/// An 800 machine, no extra upfront cost, 80/yr maintenance, a 5-year
/// life at 50% uptime, 160W, and a 1.3 buffer. Each field falls back to
/// its value here when a profile omits it, so a profile can state only
/// what its owner actually knows. Tabulated in
/// `docs/reference/pricing.md`.
impl Default for WearProfile {
    fn default() -> Self {
        WearProfile {
            purchase_price: DEFAULT_PURCHASE_PRICE,
            additional_upfront_cost: DEFAULT_ADDITIONAL_UPFRONT_COST,
            estimated_life_years: DEFAULT_LIFE_YEARS,
            estimated_uptime_percent: DEFAULT_UPTIME_PERCENT,
            annual_maintenance_cost: DEFAULT_ANNUAL_MAINTENANCE_COST,
            power_consumption_watts: DEFAULT_POWER_CONSUMPTION_WATTS,
            cost_buffer_factor: DEFAULT_COST_BUFFER_FACTOR,
            print_time_rate_override: None,
        }
    }
}

impl WearProfile {
    /// The documented defaults — same as [`WearProfile::default`], as a
    /// starting point for the `with_*` setters.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets what the printer itself cost.
    pub fn with_purchase_price(mut self, value: f64) -> Self {
        self.purchase_price = value;
        self
    }

    /// Sets anything else spent getting it running.
    pub fn with_additional_upfront_cost(mut self, value: f64) -> Self {
        self.additional_upfront_cost = value;
        self
    }

    /// Sets the expected working lifetime, in years.
    pub fn with_estimated_life_years(mut self, value: f64) -> Self {
        self.estimated_life_years = value;
        self
    }

    /// Sets the fraction of the year (`0.0`-`1.0`) spent printing.
    pub fn with_estimated_uptime_percent(mut self, value: f64) -> Self {
        self.estimated_uptime_percent = value;
        self
    }

    /// Sets the expected yearly maintenance spend.
    pub fn with_annual_maintenance_cost(mut self, value: f64) -> Self {
        self.annual_maintenance_cost = value;
        self
    }

    /// Sets the power draw while printing, in watts.
    pub fn with_power_consumption_watts(mut self, value: f64) -> Self {
        self.power_consumption_watts = value;
        self
    }

    /// Sets the multiplier over the computed machine rate.
    pub fn with_cost_buffer_factor(mut self, value: f64) -> Self {
        self.cost_buffer_factor = value;
        self
    }

    /// Sets a flat per-hour rate that replaces the computed machine
    /// rate entirely.
    pub fn with_print_time_rate_override(mut self, value: Option<f64>) -> Self {
        self.print_time_rate_override = value;
        self
    }
}

impl PrinterProfile {
    /// Builds a profile from its parts.
    pub fn new(name: impl Into<String>, kind: PrinterKind, wear: WearProfile) -> Self {
        PrinterProfile {
            name: name.into(),
            kind,
            wear,
        }
    }

    /// Serializes this profile to the TOML the profile library saves and
    /// [`crate::parse_printer`] reads back.
    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every numeric field carries a default, so a profile that states
    /// only what it knows still costs a job.
    #[test]
    fn omitted_fields_take_the_documented_defaults() {
        let profile: PrinterProfile = toml::from_str(
            r#"
name = "sparse"
kind = "fdm"
purchase_price = 699.0
"#,
        )
        .unwrap();
        assert_eq!(profile.wear.purchase_price, 699.0);
        assert_eq!(
            profile.wear,
            WearProfile {
                purchase_price: 699.0,
                ..WearProfile::default()
            }
        );
        assert_eq!(profile.wear.annual_maintenance_cost, 80.0);
        assert_eq!(profile.wear.estimated_life_years, 5.0);
        assert_eq!(profile.wear.estimated_uptime_percent, 0.50);
        assert_eq!(profile.wear.power_consumption_watts, 160.0);
        assert_eq!(profile.wear.cost_buffer_factor, 1.3);
        assert_eq!(profile.wear.additional_upfront_cost, 0.0);
        assert_eq!(profile.wear.print_time_rate_override, None);
    }

    /// A typo in a wear field has to be visible: it silently takes the
    /// default otherwise, and a printer quietly costing 800 when you
    /// wrote 699 is exactly the mistake that never gets caught. The
    /// hand-rolled `Deserialize` exists for this — `#[serde(flatten)]`
    /// hid these.
    #[test]
    fn a_misspelled_wear_field_is_reported_as_unknown() {
        let (profile, unknown) = crate::parse_toml::<PrinterProfile>(
            r#"
name = "typo"
kind = "fdm"
purchase_pric = 699.0
buffer = 1.2
"#,
        )
        .unwrap();
        let mut unknown = unknown;
        unknown.sort();
        assert_eq!(unknown, ["buffer", "purchase_pric"]);
        assert_eq!(profile.wear.purchase_price, DEFAULT_PURCHASE_PRICE);
    }

    /// Hand-rolling `Deserialize` must not change the file format the
    /// derived `Serialize` still writes.
    #[test]
    fn round_trips_through_its_own_toml() {
        let profile = PrinterProfile {
            name: "a1".into(),
            kind: PrinterKind::Fdm,
            wear: WearProfile {
                purchase_price: 699.0,
                print_time_rate_override: Some(0.5),
                ..WearProfile::default()
            },
        };
        let text = profile.to_toml().unwrap();
        let (back, unknown) = crate::parse_toml::<PrinterProfile>(&text).unwrap();
        assert_eq!(back, profile);
        assert!(unknown.is_empty(), "own output has no unknown keys");
        // Still flat — no `[wear]` table.
        assert!(text.contains("\npurchase_price = 699.0"), "{text}");
    }
}
