//! `CostBreakdown`, `SuggestedPrice`, and the cost calculation itself.
//!
//! The arithmetic here follows `PRICING.md` at the repository root, which
//! is the language-agnostic statement of the model; the names differ
//! (`efficiency_factor` for its `markup_multiplier`, `hardware` for its
//! `extra_costs`, and machine cost kept split into its wear and
//! electricity halves), the numbers don't.

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use super::filament::{MaterialKind, MaterialProfile};
use super::job::Job;
use super::printer::{PrinterKind, PrinterProfile};
use crate::formats::slice_summary::{PlateSummary, SliceSummary, SourceFormat};

/// Decimal places every cost figure is rounded to when the breakdown is
/// assembled. Intermediate arithmetic runs at full precision.
const COST_DP: i32 = 4;
/// Decimal places suggested sale prices are rounded to.
const PRICE_DP: i32 = 2;

/// Hours in a year, the basis for turning an uptime percentage into
/// printing hours.
const HOURS_PER_YEAR: f64 = 8760.0;

/// Fallback values for job fields left unset — electricity price, labor
/// rate, margin percentages. Passed to [`calculate_cost`] by the caller,
/// which owns where they come from (the CLI loads them from
/// `~/.config/printcalc/config.toml`; anything without a filesystem, like
/// `printcalc-wasm`, just uses [`CostDefaults::default`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostDefaults {
    /// Fallback for `Job::electricity_cost_per_kwh_override`.
    pub electricity_cost_per_kwh: f64,
    /// Fallback for `LaborCost::rate_per_hour`.
    pub labor_rate_per_hour: f64,
    /// Fallback for `Job::margin_percentages_override`. Default `[50.0,
    /// 60.0, 70.0]`.
    ///
    /// These are gross *margins* — the share of the sale price that isn't
    /// cost — not markups. See [`SuggestedPrice`]. The `markup_percentages`
    /// deserialize alias keeps configs written before the rename loading;
    /// their numbers now mean margins, which is a real change in what the
    /// tool prints.
    #[serde(alias = "markup_percentages")]
    pub margin_percentages: Vec<f64>,
}

impl Default for CostDefaults {
    fn default() -> Self {
        CostDefaults {
            electricity_cost_per_kwh: 0.14,
            labor_rate_per_hour: 15.0,
            margin_percentages: vec![50.0, 60.0, 70.0],
        }
    }
}

/// The full cost breakdown for a job, returned by [`calculate_cost`].
///
/// Every money field is for the **whole batch** — `quantity` is already
/// folded into all five cost components. `unit_cost` and
/// [`SuggestedPrice::unit_price`] are the per-unit views.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CostBreakdown {
    /// How many units this batch covers — `job.quantity`, copied through.
    pub quantity: u32,
    /// One entry per selected plate, in `plate_index` order. Raw slicer
    /// numbers: not efficiency-factor-adjusted like `MaterialLine::amount`,
    /// and not multiplied by `quantity` — this describes one pass of what
    /// the slicer reported, not what the batch costs.
    pub plate_breakdown: Vec<PlateLine>,
    /// Print time across every selected plate, for one unit.
    pub print_time_seconds: f64,
    /// What an hour of printing costs on this printer: depreciation plus
    /// electricity, with the profile's buffer factor applied — or
    /// `print_time_rate_override` verbatim when the profile sets one.
    pub print_time_rate: f64,
    /// Total material cost across every selected plate/slot.
    pub material_cost: f64,
    /// `material_cost` split by contributing filament profile name — one
    /// entry per distinct profile, sorted by name. A job where every slot
    /// happens to share one profile still collapses back to a single entry
    /// here.
    pub material_breakdown: Vec<MaterialLine>,
    /// Printer depreciation cost for the selected print time.
    pub wear_cost: f64,
    /// Electricity cost for the selected print time.
    pub electricity_cost: f64,
    /// `job.labor.minutes` (per unit) priced at the effective hourly rate.
    pub labor_cost: f64,
    /// Total of every `job.hardware` item.
    pub hardware_cost: f64,
    /// `hardware_cost` split by item, one entry per `job.hardware` entry
    /// in order (unlike `material_breakdown`, not aggregated — each
    /// `HardwareItem` is already its own line item, not a slot resolved
    /// against a shared profile).
    pub hardware_breakdown: Vec<HardwareLine>,
    /// Sum of every cost field above — the batch's landed cost. Computed
    /// from the unrounded components, so the rounded components may not
    /// add up to it exactly in the last decimal.
    pub total_cost: f64,
    /// `total_cost / quantity`.
    pub unit_cost: f64,
    /// One suggested sale price per configured margin percentage.
    pub suggested_prices: Vec<SuggestedPrice>,
}

/// One selected plate's raw print-time/material numbers, as reported by
/// the slicer. See [`CostBreakdown::plate_breakdown`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PlateLine {
    /// Matches the slicer's own plate numbering (1-based).
    pub plate_index: u32,
    /// Copied from `PlateSummary::print_time_seconds`.
    pub print_time_seconds: f64,
    /// Sum of every `FilamentUsage::grams` on this plate (0.0 if none).
    pub grams: f64,
    /// Sum of every `FilamentUsage::ml` on this plate (0.0 if none).
    pub ml: f64,
}

/// One filament profile's contribution to `material_cost`. See
/// [`CostBreakdown::material_breakdown`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MaterialLine {
    /// The contributing [`super::filament::MaterialProfile::name`].
    pub filament_name: String,
    /// Total amount used across the batch (after the efficiency factor
    /// and `quantity`), in `unit`.
    pub amount: f64,
    /// "g" or "ml" — whichever unit the sliced file reported the usage in
    /// (grams for FDM formats, ml for resin).
    pub unit: &'static str,
    /// Price per 1000 `unit` — the profile's own
    /// [`super::filament::MaterialProfile::price`] (per kg for filament,
    /// per liter for resin), not multiplied by `amount`.
    pub unit_price: f64,
    /// `amount` priced at `unit_price`.
    pub cost: f64,
}

/// One `job.hardware` item's contribution to `hardware_cost`. See
/// [`CostBreakdown::hardware_breakdown`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HardwareLine {
    /// Copied from `super::job::HardwareItem::name`.
    pub name: String,
    /// How many were used **per unit** — `HardwareItem::quantity`.
    pub quantity: u32,
    /// Cost per piece — `super::job::HardwareItem::cost` copied through.
    pub unit_cost: f64,
    /// `unit_cost * quantity * job.quantity`.
    pub cost: f64,
}

/// One margin tier's suggested sale price. See
/// [`CostBreakdown::suggested_prices`].
///
/// Prices come from gross **margin**, not markup: a margin is the share of
/// the sale price that isn't cost, so `price = cost / (1 - margin)`. A 60%
/// margin is `cost / 0.40` = 2.5x cost, not the 1.6x that a 60% *markup*
/// would give (which is only a 37.5% margin). The two are easy to confuse
/// and differ a lot.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct SuggestedPrice {
    /// e.g. `60.0` for a 60% gross margin.
    pub margin_percent: f64,
    /// Sale price for the whole batch: `total_cost / (1 - margin)`.
    pub price: f64,
    /// `price / quantity` — the price of one unit.
    pub unit_price: f64,
}

/// A slot the sliced file used but the job priced nothing for, with
/// whatever the file said about it. Payload of
/// [`CalcError::MissingMaterial`].
#[derive(Debug, Clone, PartialEq)]
pub struct UnmappedSlot {
    /// The slot's format-native id.
    pub slot_id: String,
    /// The slicer's filament profile name for the slot, if it gave one.
    pub profile_name: Option<String>,
    /// The material family the slicer reported, e.g. `"PLA"`.
    pub material_type: Option<String>,
    /// The color the slicer reported, usually a hex code.
    pub color: Option<String>,
}

impl fmt::Display for UnmappedSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "slot {}", self.slot_id)?;
        let described: Vec<&str> = [
            self.profile_name.as_deref(),
            self.material_type.as_deref(),
            self.color.as_deref(),
        ]
        .into_iter()
        .flatten()
        .filter(|s| !s.is_empty())
        .collect();
        if !described.is_empty() {
            write!(f, " ({})", described.join(" "))?;
        }
        Ok(())
    }
}

/// Why [`calculate_cost`] couldn't cost a job.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum CalcError {
    /// `job.plate_selection` named a plate that isn't in the sliced file.
    PlateNotFound(u32),
    /// One or more slots had nonzero usage but neither an entry in
    /// `job.material` nor a `job.default_material` to fall back on. Every
    /// unpriced slot is listed, described as far as the sliced file
    /// described it — the calc never silently prices a slot at zero.
    MissingMaterial(Vec<UnmappedSlot>),
    /// `printer.kind` doesn't match what the sliced file's format implies
    /// (e.g. a resin `.sl1s` costed against an FDM printer profile).
    /// Caught up front so it doesn't surface later as a confusing
    /// missing-price error instead.
    PrinterKindMismatch {
        /// The printer profile's name.
        printer_name: String,
        /// The printer profile's actual `kind`.
        printer_kind: PrinterKind,
        /// The sliced file's format, which implied a different kind.
        format: SourceFormat,
    },
    /// Same as `PrinterKindMismatch`, for a filament profile's `kind`.
    FilamentKindMismatch {
        /// The filament profile's name.
        filament_name: String,
        /// The filament profile's actual `kind`.
        filament_kind: MaterialKind,
        /// The sliced file's format, which implied a different kind.
        format: SourceFormat,
    },
    /// A margin percentage was 100 or more, which no finite sale price
    /// satisfies (`price = cost / (1 - margin)`). Usually a markup
    /// mistaken for a margin.
    InvalidMargin(f64),
    /// `job.quantity` was zero — there's no batch to price.
    ZeroQuantity,
}

/// The [`PrinterKind`] a sliced file's format implies. Public so callers
/// building a job from a file alone ([`crate::auto_job`]) can restrict
/// their search to profiles the format could actually have come from.
pub fn printer_kind_for(format: SourceFormat) -> PrinterKind {
    expected_printer_kind(format)
}

/// The [`MaterialKind`] a sliced file's format implies — see
/// [`printer_kind_for`].
pub fn material_kind_for(format: SourceFormat) -> MaterialKind {
    expected_material_kind(format)
}

/// The `PrinterKind` a sliced file's format implies — only `prusa-sl1` is
/// resin today; every other format is FDM.
fn expected_printer_kind(format: SourceFormat) -> PrinterKind {
    match format {
        SourceFormat::PrusaSl1 => PrinterKind::Resin,
        SourceFormat::Bambu3mf | SourceFormat::Gcode | SourceFormat::PrusaBgcode => {
            PrinterKind::Fdm
        }
    }
}

/// The `MaterialKind` a sliced file's format implies — see
/// [`expected_printer_kind`].
fn expected_material_kind(format: SourceFormat) -> MaterialKind {
    match format {
        SourceFormat::PrusaSl1 => MaterialKind::Resin,
        SourceFormat::Bambu3mf | SourceFormat::Gcode | SourceFormat::PrusaBgcode => {
            MaterialKind::Filament
        }
    }
}

impl fmt::Display for CalcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CalcError::PlateNotFound(i) => write!(f, "plate {i} not found in sliced file"),
            CalcError::MissingMaterial(slots) => {
                let listed: Vec<String> = slots.iter().map(|s| s.to_string()).collect();
                write!(
                    f,
                    "no material profile for {} — map {} explicitly, or set a default material",
                    listed.join(", "),
                    if slots.len() == 1 { "it" } else { "them" }
                )
            }
            CalcError::PrinterKindMismatch {
                printer_name,
                printer_kind,
                format,
            } => write!(
                f,
                "printer {printer_name:?} is {printer_kind:?}, but {format:?} sliced files are {:?} prints",
                expected_printer_kind(*format)
            ),
            CalcError::FilamentKindMismatch {
                filament_name,
                filament_kind,
                format,
            } => write!(
                f,
                "filament profile {filament_name:?} is {filament_kind:?}, but {format:?} sliced files need {:?}",
                expected_material_kind(*format)
            ),
            CalcError::InvalidMargin(m) => write!(
                f,
                "margin of {m}% has no finite price — a margin is the share of the sale price that isn't cost, so it must be under 100% (a 60% markup is a 37.5% margin)"
            ),
            CalcError::ZeroQuantity => write!(f, "quantity is 0 — nothing to price"),
        }
    }
}

impl std::error::Error for CalcError {}

/// Rounds `value` to `dp` decimal places. Only ever applied when
/// assembling the result; everything upstream stays at full precision.
fn round_to(value: f64, dp: i32) -> f64 {
    let scale = 10_f64.powi(dp);
    (value * scale).round() / scale
}

/// Cost breakdown for `job` against the already-parsed `slice`, using
/// already-resolved profiles (a [`super::job::ProfileRef::Linked`] must be
/// resolved against the profile library — or, for WASM, supplied directly
/// by the caller — before reaching here).
///
/// `material_profiles` is keyed by
/// [`crate::formats::slice_summary::FilamentUsage::slot_id`];
/// `default_material` is the resolved `job.default_material`, used for any
/// used slot the map doesn't cover. A slot covered by neither is an error,
/// never a free slot.
///
/// Everything returned is a batch figure — see [`CostBreakdown`].
pub fn calculate_cost(
    job: &Job,
    slice: &SliceSummary,
    printer: &PrinterProfile,
    material_profiles: &HashMap<String, MaterialProfile>,
    default_material: Option<&MaterialProfile>,
    defaults: &CostDefaults,
) -> Result<CostBreakdown, CalcError> {
    if job.quantity == 0 {
        return Err(CalcError::ZeroQuantity);
    }
    let quantity = job.quantity as f64;

    let format = slice.source_format;
    if printer.kind != expected_printer_kind(format) {
        return Err(CalcError::PrinterKindMismatch {
            printer_name: printer.name.clone(),
            printer_kind: printer.kind,
            format,
        });
    }
    for profile in material_profiles.values().chain(default_material) {
        if profile.kind != expected_material_kind(format) {
            return Err(CalcError::FilamentKindMismatch {
                filament_name: profile.name.clone(),
                filament_kind: profile.kind,
                format,
            });
        }
    }

    let selected_plates: Vec<&PlateSummary> = match job.plate_selection {
        None => slice.plates.iter().collect(),
        Some(i) => vec![
            slice
                .plates
                .iter()
                .find(|p| p.plate_index == i)
                .ok_or(CalcError::PlateNotFound(i))?,
        ],
    };
    let total_print_time_seconds: f64 = selected_plates.iter().map(|p| p.print_time_seconds).sum();

    // fold from 0.0, not `.sum()`: f64's Sum impl folds from -0.0, which
    // would print as "-0.00g"/"-0.00ml" for a plate with none of that unit.
    let mut plate_breakdown: Vec<PlateLine> = selected_plates
        .iter()
        .map(|p| PlateLine {
            plate_index: p.plate_index,
            print_time_seconds: p.print_time_seconds,
            grams: p
                .filament_usage
                .iter()
                .fold(0.0, |acc, u| acc + u.grams.unwrap_or(0.0)),
            ml: p
                .filament_usage
                .iter()
                .fold(0.0, |acc, u| acc + u.ml.unwrap_or(0.0)),
        })
        .collect();
    plate_breakdown.sort_by_key(|p| p.plate_index);

    // Every used slot has to be priced before anything is: reporting all
    // of them at once beats making the user rerun per slot.
    let mut unmapped: Vec<UnmappedSlot> = Vec::new();
    if default_material.is_none() {
        for slot in slice.slots() {
            if !material_profiles.contains_key(&slot.slot_id) {
                unmapped.push(UnmappedSlot {
                    slot_id: slot.slot_id,
                    profile_name: slot.profile_name,
                    material_type: slot.material_type,
                    color: slot.color,
                });
            }
        }
    }
    if !unmapped.is_empty() {
        return Err(CalcError::MissingMaterial(unmapped));
    }

    let mut material_cost = 0.0;
    let mut lines_by_material: HashMap<String, MaterialLine> = HashMap::new();
    for plate in &selected_plates {
        for usage in &plate.filament_usage {
            // grams or ml, whichever the usage entry actually carries; a
            // slot with zero of both is present but unused (Bambu AMS tray
            // / Prusa extruder not loaded) and is skipped.
            let native_amount = match (
                usage.grams.filter(|&g| g != 0.0),
                usage.ml.filter(|&m| m != 0.0),
            ) {
                (Some(g), _) => g,
                (None, Some(ml)) => ml,
                (None, None) => continue,
            };
            let profile = material_profiles
                .get(&usage.slot_id)
                .or(default_material)
                .expect("every used slot was checked above");
            // The profile's kind fixes the usage's unit and the price's
            // basis (per kg for filament, per liter for resin) — the
            // up-front kind checks above guarantee they match the sliced
            // file's format, so no unit conversion is ever needed.
            let unit = match profile.kind {
                MaterialKind::Filament => "g",
                MaterialKind::Resin => "ml",
            };
            let effective_factor = job
                .material_efficiency_factor_override
                .unwrap_or(profile.efficiency_factor);
            let used_amount = native_amount * effective_factor * quantity;
            let cost = used_amount * (profile.price / 1000.0);
            material_cost += cost;

            let line = lines_by_material
                .entry(profile.name.clone())
                .or_insert(MaterialLine {
                    filament_name: profile.name.clone(),
                    amount: 0.0,
                    unit,
                    unit_price: profile.price,
                    cost: 0.0,
                });
            line.amount += used_amount;
            line.cost += cost;
        }
    }
    let mut material_breakdown: Vec<MaterialLine> = lines_by_material
        .into_values()
        .map(|line| MaterialLine {
            amount: round_to(line.amount, COST_DP),
            cost: round_to(line.cost, COST_DP),
            ..line
        })
        .collect();
    material_breakdown.sort_by(|a, b| a.filament_name.cmp(&b.filament_name));

    let wear = &printer.wear;
    let total_lifetime_hours =
        wear.estimated_life_years * HOURS_PER_YEAR * wear.estimated_uptime_percent;
    // A 0-year life or 0% uptime is a profile that says "don't charge for
    // depreciation", not a division error.
    let capital_cost_per_hour = if total_lifetime_hours == 0.0 {
        0.0
    } else {
        let lifetime_cost = wear.purchase_price
            + wear.additional_upfront_cost
            + wear.annual_maintenance_cost * wear.estimated_life_years;
        lifetime_cost / total_lifetime_hours
    };

    let electricity_rate = job
        .electricity_cost_per_kwh_override
        .unwrap_or(defaults.electricity_cost_per_kwh);
    let electrical_cost_per_hour = (wear.power_consumption_watts / 1000.0) * electricity_rate;

    // An override replaces the whole computed machine rate. Splitting it
    // back into wear and electricity would be inventing a division the
    // user didn't state, so it's reported entirely as wear.
    let (wear_rate, electricity_rate_per_hour) = match wear.print_time_rate_override {
        Some(rate) => (rate, 0.0),
        None => (
            capital_cost_per_hour * wear.cost_buffer_factor,
            electrical_cost_per_hour * wear.cost_buffer_factor,
        ),
    };
    let print_time_rate = wear_rate + electricity_rate_per_hour;

    let print_hours = total_print_time_seconds / 3600.0;
    let wear_cost = wear_rate * print_hours * quantity;
    let electricity_cost = electricity_rate_per_hour * print_hours * quantity;

    let effective_labor_rate = job
        .labor
        .rate_per_hour
        .unwrap_or(defaults.labor_rate_per_hour);
    let labor_cost = effective_labor_rate * (job.labor.minutes / 60.0) * quantity;

    // Explicit loop, not `.sum()`: f64's `Sum` impl folds from -0.0, so an
    // empty `hardware` list (the common case) would otherwise leave
    // `hardware_cost` as negative zero and print "$-0.00".
    let mut hardware_cost = 0.0;
    let hardware_breakdown: Vec<HardwareLine> = job
        .hardware
        .iter()
        .map(|h| {
            let cost = h.cost * h.quantity as f64 * quantity;
            hardware_cost += cost;
            HardwareLine {
                name: h.name.clone(),
                quantity: h.quantity,
                unit_cost: h.cost,
                cost: round_to(cost, COST_DP),
            }
        })
        .collect();

    let total_cost = material_cost + wear_cost + electricity_cost + labor_cost + hardware_cost;

    let margin_percentages = job
        .margin_percentages_override
        .as_deref()
        .unwrap_or(&defaults.margin_percentages);
    let suggested_prices = margin_percentages
        .iter()
        .map(|&margin_percent| {
            let remainder = 1.0 - margin_percent / 100.0;
            if remainder <= 0.0 {
                return Err(CalcError::InvalidMargin(margin_percent));
            }
            let price = total_cost / remainder;
            Ok(SuggestedPrice {
                margin_percent,
                price: round_to(price, PRICE_DP),
                unit_price: round_to(price / quantity, PRICE_DP),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(CostBreakdown {
        quantity: job.quantity,
        plate_breakdown,
        print_time_seconds: total_print_time_seconds,
        print_time_rate: round_to(print_time_rate, COST_DP),
        material_cost: round_to(material_cost, COST_DP),
        material_breakdown,
        wear_cost: round_to(wear_cost, COST_DP),
        electricity_cost: round_to(electricity_cost, COST_DP),
        labor_cost: round_to(labor_cost, COST_DP),
        hardware_cost: round_to(hardware_cost, COST_DP),
        hardware_breakdown,
        total_cost: round_to(total_cost, COST_DP),
        unit_cost: round_to(total_cost / quantity, COST_DP),
        suggested_prices,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::slice_summary::{FilamentUsage, PlateSummary, SliceMeta, SourceFormat};
    use crate::model::job::{HardwareItem, JobSource, LaborCost, ProfileRef, SINGLE_MATERIAL_SLOT};
    use crate::model::printer::{PrinterKind, WearProfile};

    fn bambu_a1() -> PrinterProfile {
        PrinterProfile {
            name: "bambu-a1".into(),
            kind: PrinterKind::Fdm,
            wear: WearProfile {
                purchase_price: 800.0,
                additional_upfront_cost: 0.0,
                estimated_life_years: 5.0,
                estimated_uptime_percent: 0.5,
                annual_maintenance_cost: 80.0,
                power_consumption_watts: 160.0,
                cost_buffer_factor: 1.3,
                print_time_rate_override: None,
            },
        }
    }

    fn filament(name: &str, price: f64) -> MaterialProfile {
        MaterialProfile {
            name: name.into(),
            kind: MaterialKind::Filament,
            price,
            efficiency_factor: 1.1,
            color: None,
            vendor: None,
        }
    }

    fn resin_printer() -> PrinterProfile {
        PrinterProfile {
            name: "sl1-printer".into(),
            kind: PrinterKind::Resin,
            wear: bambu_a1().wear,
        }
    }

    /// Like [`filament`], but `kind: Resin` and priced per liter.
    fn resin(name: &str, price: f64) -> MaterialProfile {
        MaterialProfile {
            kind: MaterialKind::Resin,
            ..filament(name, price)
        }
    }

    fn usage(slot_id: &str, grams: f64) -> FilamentUsage {
        FilamentUsage {
            slot_id: slot_id.into(),
            material_type: Some("PLA".into()),
            grams: Some(grams),
            ..FilamentUsage::default()
        }
    }

    fn plate(index: u32, seconds: f64, filament_usage: Vec<FilamentUsage>) -> PlateSummary {
        PlateSummary {
            plate_index: index,
            print_time_seconds: seconds,
            filament_usage,
            printer_model: None,
            layer_height: None,
            extra: HashMap::new(),
        }
    }

    fn slice(format: SourceFormat, plates: Vec<PlateSummary>) -> SliceSummary {
        SliceSummary {
            source_format: format,
            meta: SliceMeta::default(),
            plates,
        }
    }

    fn base_job(material: HashMap<String, ProfileRef<MaterialProfile>>, labor_minutes: f64) -> Job {
        Job {
            name: "test".into(),
            source: JobSource::Path("test.gcode.3mf".into()),
            plate_selection: None,
            printer: ProfileRef::Linked {
                name: "bambu-a1".into(),
            },
            material,
            default_material: None,
            quantity: 1,
            material_efficiency_factor_override: None,
            electricity_cost_per_kwh_override: None,
            labor: LaborCost {
                rate_per_hour: Some(15.0),
                minutes: labor_minutes,
            },
            hardware: Vec::new(),
            margin_percentages_override: None,
        }
    }

    fn linked(name: &str) -> ProfileRef<MaterialProfile> {
        ProfileRef::Linked { name: name.into() }
    }

    /// Reference worksheet example: 800 dollar printer, 5yr life, 50%
    /// uptime, 160W, 0.14/kWh, 1.3 buffer.
    #[test]
    fn wear_and_electricity_rate_per_hour() {
        let wear = &bambu_a1().wear;
        let capital_cost_per_hour = (wear.purchase_price
            + wear.annual_maintenance_cost * wear.estimated_life_years)
            / (wear.estimated_life_years * HOURS_PER_YEAR * wear.estimated_uptime_percent);
        let electrical_cost_per_hour = (wear.power_consumption_watts / 1000.0) * 0.14;
        assert!((capital_cost_per_hour - 0.0548).abs() < 1e-3);
        assert!((electrical_cost_per_hour - 0.0224).abs() < 1e-4);
        let combined = (capital_cost_per_hour + electrical_cost_per_hour) * wear.cost_buffer_factor;
        assert!((combined - 0.10).abs() < 5e-3);
    }

    /// The machine-rate worked example from `PRICING.md` §1, checked
    /// through the public API rather than by restating its arithmetic:
    /// `C=699, A=0, M=80, Y=5, U=0.50, P=105, B=1.3, E=0.14` gives
    /// `R = 0.084347`, which reaches `print_time_rate` rounded to 4dp.
    #[test]
    fn pricing_md_machine_rate_example() {
        let printer = PrinterProfile {
            name: "prusa-p1s".into(),
            kind: PrinterKind::Fdm,
            wear: WearProfile {
                purchase_price: 699.0,
                additional_upfront_cost: 0.0,
                estimated_life_years: 5.0,
                estimated_uptime_percent: 0.50,
                annual_maintenance_cost: 80.0,
                power_consumption_watts: 105.0,
                cost_buffer_factor: 1.3,
                print_time_rate_override: None,
            },
        };
        let slice = slice(
            SourceFormat::Bambu3mf,
            vec![plate(1, 3600.0, vec![usage(SINGLE_MATERIAL_SLOT, 10.0)])],
        );
        let breakdown = calculate_cost(
            &base_job(
                HashMap::from([(SINGLE_MATERIAL_SLOT.to_string(), linked("pla-black"))]),
                0.0,
            ),
            &slice,
            &printer,
            &HashMap::from([(
                SINGLE_MATERIAL_SLOT.to_string(),
                filament("pla-black", 18.99),
            )]),
            None,
            &CostDefaults::default(),
        )
        .unwrap();

        assert_eq!(breakdown.print_time_rate, 0.0843);
        // One hour of printing, so the machine cost is the rate itself,
        // split into the depreciation and electricity halves the display
        // keeps apart. Both carry the buffer factor.
        assert!((breakdown.wear_cost - 0.050183 * 1.3).abs() < 1e-4);
        assert!((breakdown.electricity_cost - 0.0147 * 1.3).abs() < 1e-4);
        assert!((breakdown.wear_cost + breakdown.electricity_cost - 0.084347).abs() < 1e-4);
    }

    /// Multi-plate worked example: two Bambu plates, single filament.
    #[test]
    fn multi_plate_single_filament_matches_worked_example() {
        let slice = slice(
            SourceFormat::Bambu3mf,
            vec![
                plate(1, 1037.0, vec![usage(SINGLE_MATERIAL_SLOT, 6.66)]),
                plate(2, 988.0, vec![usage(SINGLE_MATERIAL_SLOT, 5.15)]),
            ],
        );
        let material_profiles = HashMap::from([(
            SINGLE_MATERIAL_SLOT.to_string(),
            filament("pla-black", 18.99),
        )]);
        let job = base_job(
            HashMap::from([(SINGLE_MATERIAL_SLOT.to_string(), linked("pla-black"))]),
            10.0,
        );

        let breakdown = calculate_cost(
            &job,
            &slice,
            &bambu_a1(),
            &material_profiles,
            None,
            &CostDefaults::default(),
        )
        .unwrap();

        assert_eq!(breakdown.quantity, 1);
        assert_eq!(
            breakdown.plate_breakdown,
            vec![
                PlateLine {
                    plate_index: 1,
                    print_time_seconds: 1037.0,
                    grams: 6.66,
                    ml: 0.0
                },
                PlateLine {
                    plate_index: 2,
                    print_time_seconds: 988.0,
                    grams: 5.15,
                    ml: 0.0
                },
            ]
        );

        assert!((breakdown.material_cost - 0.2467).abs() < 1e-3);
        assert_eq!(breakdown.material_breakdown.len(), 1);
        let line = &breakdown.material_breakdown[0];
        assert_eq!(line.filament_name, "pla-black");
        assert!((line.cost - breakdown.material_cost).abs() < 1e-9);
        assert_eq!(line.unit, "g");
        assert_eq!(line.unit_price, 18.99);
        // (6.66 + 5.15)g * 1.1 efficiency factor
        assert!((line.amount - 12.991).abs() < 1e-4);
        assert!((breakdown.wear_cost - 0.0401).abs() < 1e-3);
        assert!((breakdown.electricity_cost - 0.0164).abs() < 1e-3);
        assert_eq!(breakdown.labor_cost, 2.5);
        assert_eq!(breakdown.hardware_cost, 0.0);
        assert!(
            breakdown.hardware_cost.is_sign_positive(),
            "hardware_cost must not be -0.0"
        );
        assert!(breakdown.hardware_breakdown.is_empty());
        assert!((breakdown.total_cost - 2.8031).abs() < 1e-3);
        assert_eq!(breakdown.unit_cost, breakdown.total_cost);
        assert_eq!(breakdown.print_time_seconds, 2025.0);
        assert!((breakdown.print_time_rate - 0.1005).abs() < 1e-3);
    }

    /// Prices come from gross margin, not markup: a 60% margin is
    /// `cost / 0.40`, i.e. 2.5x cost — not the 1.6x a 60% markup gives.
    #[test]
    fn suggested_prices_are_margins_not_markups() {
        let slice = slice(
            SourceFormat::Bambu3mf,
            vec![plate(1, 3600.0, vec![usage("1", 100.0)])],
        );
        let mut job = base_job(HashMap::from([("1".to_string(), linked("pla"))]), 0.0);
        job.margin_percentages_override = Some(vec![50.0, 60.0, 70.0]);
        let profiles = HashMap::from([("1".to_string(), filament("pla", 20.0))]);

        let breakdown = calculate_cost(
            &job,
            &slice,
            &bambu_a1(),
            &profiles,
            None,
            &CostDefaults::default(),
        )
        .unwrap();

        let cost = breakdown.total_cost;
        let prices = &breakdown.suggested_prices;
        assert_eq!(prices.len(), 3);
        assert!((prices[0].price - cost / 0.50).abs() < 0.01);
        assert!((prices[1].price - cost / 0.40).abs() < 0.01);
        assert!((prices[2].price - cost / 0.30).abs() < 0.01);
        // ...and emphatically not the markup reading of the same numbers.
        assert!((prices[1].price - cost * 1.60).abs() > 0.01);
    }

    /// A margin of 100% or more has no finite price. It's the shape a
    /// markup percentage takes when mistaken for a margin, so it fails
    /// loudly rather than returning an infinity.
    #[test]
    fn a_margin_of_100_percent_or_more_is_an_error() {
        let slice = slice(
            SourceFormat::Bambu3mf,
            vec![plate(1, 100.0, vec![usage("1", 1.0)])],
        );
        let mut job = base_job(HashMap::from([("1".to_string(), linked("pla"))]), 0.0);
        job.margin_percentages_override = Some(vec![100.0]);
        let profiles = HashMap::from([("1".to_string(), filament("pla", 20.0))]);

        let err = calculate_cost(
            &job,
            &slice,
            &bambu_a1(),
            &profiles,
            None,
            &CostDefaults::default(),
        )
        .unwrap_err();
        assert_eq!(err, CalcError::InvalidMargin(100.0));
    }

    /// `quantity` scales all five components, so total, unit cost and
    /// prices all follow. Per-plate lines stay per-unit.
    #[test]
    fn quantity_scales_the_whole_batch() {
        let slice = slice(
            SourceFormat::Bambu3mf,
            vec![plate(1, 3600.0, vec![usage("1", 20.0)])],
        );
        let profiles = HashMap::from([("1".to_string(), filament("pla", 20.0))]);
        let one = base_job(HashMap::from([("1".to_string(), linked("pla"))]), 12.0);
        let mut ten = one.clone();
        ten.quantity = 10;
        ten.hardware = vec![HardwareItem {
            name: "magnet".into(),
            cost: 0.10,
            quantity: 2,
        }];
        let mut one = one;
        one.hardware = ten.hardware.clone();

        let defaults = CostDefaults::default();
        let single = calculate_cost(&one, &slice, &bambu_a1(), &profiles, None, &defaults).unwrap();
        let batch = calculate_cost(&ten, &slice, &bambu_a1(), &profiles, None, &defaults).unwrap();

        assert_eq!(batch.quantity, 10);
        for (batched, unit) in [
            (batch.material_cost, single.material_cost),
            (batch.wear_cost, single.wear_cost),
            (batch.electricity_cost, single.electricity_cost),
            (batch.labor_cost, single.labor_cost),
            (batch.hardware_cost, single.hardware_cost),
            (batch.total_cost, single.total_cost),
        ] {
            assert!((batched - unit * 10.0).abs() < 1e-3, "{batched} vs {unit}");
        }
        // 2 magnets per unit stays 2 on the line; only the cost scales.
        assert_eq!(batch.hardware_breakdown[0].quantity, 2);
        assert!((batch.hardware_breakdown[0].cost - 2.0).abs() < 1e-9);
        assert!((batch.unit_cost - single.total_cost).abs() < 1e-3);
        assert!(
            (batch.suggested_prices[0].unit_price - single.suggested_prices[0].price).abs() < 0.01
        );
        // Plate lines describe one pass of the slicer's numbers, not the batch.
        assert_eq!(batch.plate_breakdown[0].grams, 20.0);
        assert_eq!(batch.print_time_seconds, 3600.0);
    }

    #[test]
    fn zero_quantity_is_an_error() {
        let slice = slice(SourceFormat::Bambu3mf, vec![plate(1, 100.0, vec![])]);
        let mut job = base_job(HashMap::new(), 0.0);
        job.quantity = 0;
        let err = calculate_cost(
            &job,
            &slice,
            &bambu_a1(),
            &HashMap::new(),
            None,
            &CostDefaults::default(),
        )
        .unwrap_err();
        assert_eq!(err, CalcError::ZeroQuantity);
    }

    /// A profile with a 0-year life (or 0% uptime) charges nothing for
    /// depreciation instead of dividing by zero.
    #[test]
    fn zero_lifetime_hours_charges_no_depreciation() {
        let mut printer = bambu_a1();
        printer.wear.estimated_uptime_percent = 0.0;
        let slice = slice(SourceFormat::Bambu3mf, vec![plate(1, 3600.0, vec![])]);
        let job = base_job(HashMap::new(), 0.0);

        let breakdown = calculate_cost(
            &job,
            &slice,
            &printer,
            &HashMap::new(),
            None,
            &CostDefaults::default(),
        )
        .unwrap();
        assert_eq!(breakdown.wear_cost, 0.0);
        assert!(breakdown.electricity_cost > 0.0);
        assert!(breakdown.total_cost.is_finite());
    }

    /// `print_time_rate_override` replaces the entire computed machine
    /// rate — depreciation, electricity and buffer factor alike.
    #[test]
    fn print_time_rate_override_replaces_the_computed_rate() {
        let mut printer = bambu_a1();
        printer.wear.print_time_rate_override = Some(2.50);
        let slice = slice(SourceFormat::Bambu3mf, vec![plate(1, 7200.0, vec![])]);
        let job = base_job(HashMap::new(), 0.0);

        let breakdown = calculate_cost(
            &job,
            &slice,
            &printer,
            &HashMap::new(),
            None,
            &CostDefaults::default(),
        )
        .unwrap();
        assert_eq!(breakdown.print_time_rate, 2.50);
        assert_eq!(breakdown.wear_cost, 5.0); // 2 hours
        assert_eq!(breakdown.electricity_cost, 0.0);
    }

    /// Multi-filament worked example: one plate, three AMS slots priced
    /// against three different profiles.
    #[test]
    fn multi_slot_filament_splits_material_cost() {
        let slice = slice(
            SourceFormat::Bambu3mf,
            vec![plate(
                1,
                1360.0, // 22m40s
                vec![
                    usage("1", 6.20),
                    usage("2", 3.40),
                    FilamentUsage {
                        material_type: Some("PETG".into()),
                        ..usage("4", 1.85)
                    },
                    // Unused slot: no profile mapped, and none needed.
                    FilamentUsage {
                        material_type: None,
                        ..usage("3", 0.0)
                    },
                ],
            )],
        );
        let material_profiles = HashMap::from([
            ("1".to_string(), filament("pla-black", 18.99)),
            ("2".to_string(), filament("pla-red", 18.99)),
            ("4".to_string(), filament("petg-clear", 22.99)),
        ]);
        let job = base_job(
            HashMap::from([
                ("1".to_string(), linked("pla-black")),
                ("2".to_string(), linked("pla-red")),
                ("4".to_string(), linked("petg-clear")),
            ]),
            15.0,
        );

        let breakdown = calculate_cost(
            &job,
            &slice,
            &bambu_a1(),
            &material_profiles,
            None,
            &CostDefaults::default(),
        )
        .unwrap();

        assert!((breakdown.material_cost - 0.2474).abs() < 1e-3);
        assert_eq!(breakdown.labor_cost, 3.75);
        assert!((breakdown.total_cost - 4.0353).abs() < 1e-3);

        // Sorted by name: petg-clear, pla-black, pla-red.
        let names: Vec<&str> = breakdown
            .material_breakdown
            .iter()
            .map(|l| l.filament_name.as_str())
            .collect();
        assert_eq!(names, ["petg-clear", "pla-black", "pla-red"]);
        let sum: f64 = breakdown.material_breakdown.iter().map(|l| l.cost).sum();
        assert!((sum - breakdown.material_cost).abs() < 1e-3);
        let pla_black = breakdown
            .material_breakdown
            .iter()
            .find(|l| l.filament_name == "pla-black")
            .unwrap();
        assert!((pla_black.cost - 0.12955).abs() < 1e-3);
        assert!((pla_black.amount - 6.82).abs() < 1e-4); // 6.20g * 1.1
    }

    /// Every unpriced slot is reported at once, described as far as the
    /// sliced file described it.
    #[test]
    fn unmapped_slots_are_all_reported_with_what_the_file_said() {
        let slice = slice(
            SourceFormat::Bambu3mf,
            vec![plate(
                1,
                100.0,
                vec![
                    FilamentUsage {
                        color: Some("#FF6A13".into()),
                        ..usage("2", 5.0)
                    },
                    FilamentUsage {
                        material_type: Some("PETG".into()),
                        ..usage("4", 1.0)
                    },
                ],
            )],
        );
        let job = base_job(HashMap::from([("1".to_string(), linked("pla-black"))]), 1.0);

        let err = calculate_cost(
            &job,
            &slice,
            &bambu_a1(),
            &HashMap::new(),
            None,
            &CostDefaults::default(),
        )
        .unwrap_err();
        let CalcError::MissingMaterial(slots) = err else {
            panic!("expected MissingMaterial, got {err:?}");
        };
        assert_eq!(slots.len(), 2);
        assert_eq!(slots[0].slot_id, "2");
        assert_eq!(slots[0].color.as_deref(), Some("#FF6A13"));
        assert_eq!(slots[0].to_string(), "slot 2 (PLA #FF6A13)");
        assert_eq!(slots[1].to_string(), "slot 4 (PETG)");
    }

    /// A `default_material` covers every slot the map doesn't, so an
    /// otherwise-unmapped slot costs instead of erroring.
    #[test]
    fn default_material_covers_unmapped_slots() {
        let slice = slice(
            SourceFormat::Bambu3mf,
            vec![plate(1, 3600.0, vec![usage("1", 10.0), usage("2", 10.0)])],
        );
        let job = base_job(HashMap::from([("1".to_string(), linked("pla-black"))]), 0.0);
        let profiles = HashMap::from([("1".to_string(), filament("pla-black", 20.0))]);
        let fallback = filament("house-pla", 10.0);

        let breakdown = calculate_cost(
            &job,
            &slice,
            &bambu_a1(),
            &profiles,
            Some(&fallback),
            &CostDefaults::default(),
        )
        .unwrap();

        // 10g * 1.1 * $20/kg + 10g * 1.1 * $10/kg
        assert!((breakdown.material_cost - 0.33).abs() < 1e-9);
        assert_eq!(breakdown.material_breakdown.len(), 2);
    }

    #[test]
    fn unknown_plate_selection_is_an_error() {
        let slice = slice(SourceFormat::Bambu3mf, vec![]);
        let mut job = base_job(
            HashMap::from([(SINGLE_MATERIAL_SLOT.to_string(), linked("pla-black"))]),
            1.0,
        );
        job.plate_selection = Some(3);

        let err = calculate_cost(
            &job,
            &slice,
            &bambu_a1(),
            &HashMap::new(),
            None,
            &CostDefaults::default(),
        )
        .unwrap_err();
        assert_eq!(err, CalcError::PlateNotFound(3));
    }

    fn resin_slice(ml: f64) -> SliceSummary {
        slice(
            SourceFormat::PrusaSl1,
            vec![plate(
                1,
                3600.0,
                vec![FilamentUsage {
                    slot_id: SINGLE_MATERIAL_SLOT.into(),
                    ml: Some(ml),
                    ..FilamentUsage::default()
                }],
            )],
        )
    }

    /// A ml-reported resin usage prices straight off the profile's
    /// per-liter price — no unit conversion involved.
    #[test]
    fn ml_usage_is_priced_per_liter() {
        let material_profiles =
            HashMap::from([(SINGLE_MATERIAL_SLOT.to_string(), resin("grey-resin", 40.0))]);
        let job = base_job(
            HashMap::from([(SINGLE_MATERIAL_SLOT.to_string(), linked("grey-resin"))]),
            0.0,
        );

        let breakdown = calculate_cost(
            &job,
            &resin_slice(10.0),
            &resin_printer(),
            &material_profiles,
            None,
            &CostDefaults::default(),
        )
        .unwrap();

        // 10ml * 1.1 efficiency factor * $40/1000 per ml = $0.44.
        assert!((breakdown.material_cost - 0.44).abs() < 1e-9);
        let line = &breakdown.material_breakdown[0];
        assert_eq!(line.unit, "ml");
        assert_eq!(line.unit_price, 40.0);
    }

    /// The exact scenario a real user hit: an FDM printer profile costed
    /// against a resin (`.sl1s`) sliced file.
    #[test]
    fn fdm_printer_against_resin_slice_is_an_error() {
        let job = base_job(
            HashMap::from([(SINGLE_MATERIAL_SLOT.to_string(), linked("pla-basic"))]),
            0.0,
        );

        let err = calculate_cost(
            &job,
            &resin_slice(10.0),
            &bambu_a1(), // Fdm, but the slice is PrusaSl1 (resin)
            &HashMap::new(),
            None,
            &CostDefaults::default(),
        )
        .unwrap_err();
        assert_eq!(
            err,
            CalcError::PrinterKindMismatch {
                printer_name: "bambu-a1".into(),
                printer_kind: PrinterKind::Fdm,
                format: SourceFormat::PrusaSl1,
            }
        );
    }

    #[test]
    fn filament_kind_against_resin_slice_is_an_error() {
        let material_profiles = HashMap::from([(
            SINGLE_MATERIAL_SLOT.to_string(),
            filament("pla-basic", 20.0), // MaterialKind::Filament
        )]);
        let job = base_job(
            HashMap::from([(SINGLE_MATERIAL_SLOT.to_string(), linked("pla-basic"))]),
            0.0,
        );

        let err = calculate_cost(
            &job,
            &resin_slice(10.0),
            &resin_printer(),
            &material_profiles,
            None,
            &CostDefaults::default(),
        )
        .unwrap_err();
        assert_eq!(
            err,
            CalcError::FilamentKindMismatch {
                filament_name: "pla-basic".into(),
                filament_kind: MaterialKind::Filament,
                format: SourceFormat::PrusaSl1,
            }
        );
    }

    #[test]
    fn hardware_cost_breaks_down_by_item() {
        let slice = slice(SourceFormat::Bambu3mf, vec![plate(1, 600.0, vec![])]);
        let mut job = base_job(HashMap::new(), 0.0);
        job.hardware = vec![
            HardwareItem {
                name: "brass insert M3x5".into(),
                cost: 0.12,
                quantity: 4,
            },
            HardwareItem {
                name: "magnet 6x2mm".into(),
                cost: 0.08,
                quantity: 2,
            },
        ];

        let breakdown = calculate_cost(
            &job,
            &slice,
            &bambu_a1(),
            &HashMap::new(),
            None,
            &CostDefaults::default(),
        )
        .unwrap();

        assert_eq!(
            breakdown.hardware_breakdown,
            vec![
                HardwareLine {
                    name: "brass insert M3x5".into(),
                    quantity: 4,
                    unit_cost: 0.12,
                    cost: 0.48
                },
                HardwareLine {
                    name: "magnet 6x2mm".into(),
                    quantity: 2,
                    unit_cost: 0.08,
                    cost: 0.16
                },
            ]
        );
        assert!((breakdown.hardware_cost - 0.64).abs() < 1e-9);
    }

    /// Costs round to 4dp and prices to 2dp; the total comes from the
    /// unrounded components, so it needn't equal their rounded sum.
    #[test]
    fn results_are_rounded_when_assembled() {
        let slice = slice(
            SourceFormat::Bambu3mf,
            vec![plate(1, 1234.0, vec![usage("1", 7.77)])],
        );
        let profiles = HashMap::from([("1".to_string(), filament("pla", 18.99))]);
        let job = base_job(HashMap::from([("1".to_string(), linked("pla"))]), 7.0);

        let b = calculate_cost(
            &job,
            &slice,
            &bambu_a1(),
            &profiles,
            None,
            &CostDefaults::default(),
        )
        .unwrap();

        for value in [
            b.material_cost,
            b.wear_cost,
            b.electricity_cost,
            b.total_cost,
        ] {
            assert_eq!(value, round_to(value, COST_DP), "{value} is not 4dp");
        }
        for price in &b.suggested_prices {
            assert_eq!(price.price, round_to(price.price, PRICE_DP));
            assert_eq!(price.unit_price, round_to(price.unit_price, PRICE_DP));
        }
    }
}
