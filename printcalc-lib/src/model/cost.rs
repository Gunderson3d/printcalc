//! `CostBreakdown` and `SuggestedPrice`.

use std::collections::HashMap;
use std::fmt;

use serde::Serialize;

use super::filament::{FilamentProfile, MaterialKind};
use super::job::Job;
use super::printer::{PrinterKind, PrinterProfile};
use crate::formats::slice_summary::{PlateSummary, SliceSummary, SourceFormat};
use crate::library::GlobalConfig;

/// The full cost breakdown for a job, returned by [`calculate_cost`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CostBreakdown {
    /// One entry per selected plate, in `plate_index` order. Raw slicer
    /// numbers, not efficiency-factor-adjusted like `MaterialLine::amount`
    /// — this describes what the slicer reported per plate, not what
    /// buying enough material costs.
    pub plate_breakdown: Vec<PlateLine>,
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
    /// `job.labor.minutes` priced at the effective hourly rate.
    pub labor_cost: f64,
    /// Total of every `job.hardware` item.
    pub hardware_cost: f64,
    /// `hardware_cost` split by item, one entry per `job.hardware` entry
    /// in order (unlike `material_breakdown`, not aggregated — each
    /// `HardwareItem` is already its own line item, not a slot resolved
    /// against a shared profile).
    pub hardware_breakdown: Vec<HardwareLine>,
    /// Sum of every cost field above.
    pub total_cost: f64,
    /// One suggested sale price per configured markup percentage.
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
    /// The contributing [`super::filament::FilamentProfile::name`].
    pub filament_name: String,
    /// Total amount used (after the efficiency factor), in `unit`.
    pub amount: f64,
    /// "g" or "ml" — whichever unit `unit_price` is denominated in (the
    /// usage's own unit, or the other one if priced via a
    /// `density_g_per_cm3` conversion).
    pub unit: &'static str,
    /// Price per 1000 `unit` (i.e. `price_per_kg`/`price_per_liter` as
    /// applied) — the profile's own price, not multiplied by `amount`.
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
    /// How many were used.
    pub quantity: u32,
    /// Cost per unit — `super::job::HardwareItem::cost` copied through.
    pub unit_cost: f64,
    /// `unit_cost * quantity`.
    pub cost: f64,
}

/// One markup tier's suggested sale price. See
/// [`CostBreakdown::suggested_prices`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct SuggestedPrice {
    /// e.g. `50.0` for a 50% markup.
    pub markup_percent: f64,
    /// `total_cost * (1 + markup_percent / 100)`.
    pub price: f64,
}

/// Why [`calculate_cost`] couldn't cost a job.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum CalcError {
    /// `job.plate_selection` named a plate that isn't in the sliced file.
    PlateNotFound(u32),
    /// A `FilamentUsage` slot had nonzero usage but no entry in
    /// `job.filament`.
    MissingFilamentProfile(String),
    /// A filament profile had no price in the unit its usage was reported
    /// in (grams or ml), and no `density_g_per_cm3` to convert to the
    /// other unit it does have a price for.
    MissingFilamentPrice {
        /// The filament profile's name.
        name: String,
        /// `"grams"` or `"ml"`.
        usage_unit: &'static str,
    },
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
}

/// The `PrinterKind` a sliced file's format implies — only `prusa-sl1` is
/// resin today; every other format is FDM.
fn expected_printer_kind(format: SourceFormat) -> PrinterKind {
    match format {
        SourceFormat::PrusaSl1 => PrinterKind::Resin,
        SourceFormat::Bambu3mf | SourceFormat::PrusaGcode | SourceFormat::PrusaBgcode => PrinterKind::Fdm,
    }
}

/// The `MaterialKind` a sliced file's format implies — see
/// [`expected_printer_kind`].
fn expected_material_kind(format: SourceFormat) -> MaterialKind {
    match format {
        SourceFormat::PrusaSl1 => MaterialKind::Resin,
        SourceFormat::Bambu3mf | SourceFormat::PrusaGcode | SourceFormat::PrusaBgcode => MaterialKind::Filament,
    }
}

impl fmt::Display for CalcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CalcError::PlateNotFound(i) => write!(f, "plate {i} not found in sliced file"),
            CalcError::MissingFilamentProfile(slot) => {
                write!(f, "no filament profile mapped for slot {slot}")
            }
            CalcError::MissingFilamentPrice { name, usage_unit } => write!(
                f,
                "filament profile {name:?} has no price for its usage (reported in {usage_unit}) — \
                 set the matching price_per_kg/price_per_liter, or set density_g_per_cm3 so the \
                 other price can be converted"
            ),
            CalcError::PrinterKindMismatch { printer_name, printer_kind, format } => write!(
                f,
                "printer {printer_name:?} is {printer_kind:?}, but {format:?} sliced files are {:?} prints",
                expected_printer_kind(*format)
            ),
            CalcError::FilamentKindMismatch { filament_name, filament_kind, format } => write!(
                f,
                "filament profile {filament_name:?} is {filament_kind:?}, but {format:?} sliced files need {:?}",
                expected_material_kind(*format)
            ),
        }
    }
}

impl std::error::Error for CalcError {}

/// Cost breakdown for `job` against the already-parsed `slice`, using
/// already-resolved profiles (a [`super::job::ProfileRef::Linked`] must be
/// resolved against the profile library — or, for WASM, supplied directly
/// by the caller — before reaching here).
pub fn calculate_cost(
    job: &Job,
    slice: &SliceSummary,
    printer: &PrinterProfile,
    filament_profiles: &HashMap<String, FilamentProfile>,
    global: &GlobalConfig,
) -> Result<CostBreakdown, CalcError> {
    let format = slice.source_format;
    if printer.kind != expected_printer_kind(format) {
        return Err(CalcError::PrinterKindMismatch {
            printer_name: printer.name.clone(),
            printer_kind: printer.kind,
            format,
        });
    }
    for profile in filament_profiles.values() {
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
            grams: p.filament_usage.iter().fold(0.0, |acc, u| acc + u.grams.unwrap_or(0.0)),
            ml: p.filament_usage.iter().fold(0.0, |acc, u| acc + u.ml.unwrap_or(0.0)),
        })
        .collect();
    plate_breakdown.sort_by_key(|p| p.plate_index);

    let mut material_cost = 0.0;
    let mut lines_by_filament: HashMap<String, MaterialLine> = HashMap::new();
    for plate in &selected_plates {
        for usage in &plate.filament_usage {
            // grams or ml, whichever the usage entry actually carries; a
            // slot with zero of both is present but unused (Bambu AMS tray
            // / Prusa extruder not loaded) and is skipped.
            let (native_amount, want_grams) =
                match (usage.grams.filter(|&g| g != 0.0), usage.ml.filter(|&m| m != 0.0)) {
                    (Some(g), _) => (g, true),
                    (None, Some(ml)) => (ml, false),
                    (None, None) => continue,
                };
            let profile = filament_profiles
                .get(&usage.slot_id)
                .ok_or_else(|| CalcError::MissingFilamentProfile(usage.slot_id.clone()))?;
            // Prefer the profile's price in the usage's own unit; if that's
            // missing, fall back to converting the amount via
            // density_g_per_cm3 (1 ml = 1 cm3, so grams = ml * density) and
            // pricing it in the profile's other unit instead.
            let (amount, unit, price_per_unit) = match (
                want_grams,
                profile.price_per_kg,
                profile.price_per_liter,
                profile.density_g_per_cm3,
            ) {
                (true, Some(price), ..) => (native_amount, "g", price),
                (false, _, Some(price), _) => (native_amount, "ml", price),
                (true, None, Some(price_per_liter), Some(density)) => {
                    (native_amount / density, "ml", price_per_liter)
                }
                (false, Some(price_per_kg), None, Some(density)) => {
                    (native_amount * density, "g", price_per_kg)
                }
                _ => {
                    return Err(CalcError::MissingFilamentPrice {
                        name: profile.name.clone(),
                        usage_unit: if want_grams { "grams" } else { "ml" },
                    });
                }
            };
            let effective_factor = job
                .material_efficiency_factor_override
                .unwrap_or(profile.efficiency_factor);
            let used_amount = amount * effective_factor;
            let cost = used_amount * (price_per_unit / 1000.0);
            material_cost += cost;

            let line = lines_by_filament.entry(profile.name.clone()).or_insert(MaterialLine {
                filament_name: profile.name.clone(),
                amount: 0.0,
                unit,
                unit_price: price_per_unit,
                cost: 0.0,
            });
            line.amount += used_amount;
            line.cost += cost;
        }
    }
    let mut material_breakdown: Vec<MaterialLine> = lines_by_filament.into_values().collect();
    material_breakdown.sort_by(|a, b| a.filament_name.cmp(&b.filament_name));

    let wear = &printer.wear;
    let total_investment = wear.purchase_price + wear.additional_upfront_cost;
    let lifetime_cost = total_investment + wear.annual_maintenance_cost * wear.estimated_life_years;
    let uptime_hours_per_year = 8760.0 * wear.estimated_uptime_percent;
    let capital_cost_per_hour = lifetime_cost / (wear.estimated_life_years * uptime_hours_per_year);

    let electricity_rate = job
        .electricity_cost_per_kwh_override
        .unwrap_or(global.electricity_cost_per_kwh);
    let electrical_cost_per_hour = (wear.power_consumption_watts / 1000.0) * electricity_rate;

    let print_hours = total_print_time_seconds / 3600.0;
    let wear_cost = capital_cost_per_hour * wear.cost_buffer_factor * print_hours;
    let electricity_cost = electrical_cost_per_hour * wear.cost_buffer_factor * print_hours;

    let effective_labor_rate = job.labor.rate_per_hour.unwrap_or(global.labor_rate_per_hour);
    let labor_cost = effective_labor_rate * (job.labor.minutes / 60.0);

    // Explicit loop, not `.sum()`: f64's `Sum` impl folds from -0.0, so an
    // empty `hardware` list (the common case) would otherwise leave
    // `hardware_cost` as negative zero and print "$-0.00".
    let mut hardware_cost = 0.0;
    let hardware_breakdown: Vec<HardwareLine> = job
        .hardware
        .iter()
        .map(|h| {
            let cost = h.cost * h.quantity as f64;
            hardware_cost += cost;
            HardwareLine { name: h.name.clone(), quantity: h.quantity, unit_cost: h.cost, cost }
        })
        .collect();

    let total_cost = material_cost + wear_cost + electricity_cost + labor_cost + hardware_cost;

    let markup_percentages = job
        .markup_percentages_override
        .as_deref()
        .unwrap_or(&global.markup_percentages);
    let suggested_prices = markup_percentages
        .iter()
        .map(|&markup_percent| SuggestedPrice {
            markup_percent,
            price: total_cost * (1.0 + markup_percent / 100.0),
        })
        .collect();

    Ok(CostBreakdown {
        plate_breakdown,
        material_cost,
        material_breakdown,
        wear_cost,
        electricity_cost,
        labor_cost,
        hardware_cost,
        hardware_breakdown,
        total_cost,
        suggested_prices,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::slice_summary::{FilamentUsage, PlateSummary, SourceFormat};
    use crate::model::filament::MaterialKind;
    use crate::model::job::{HardwareItem, JobSource, LaborCost, ProfileRef, SINGLE_FILAMENT_SLOT};
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
            },
        }
    }

    fn filament(name: &str, price_per_kg: f64) -> FilamentProfile {
        FilamentProfile {
            name: name.into(),
            kind: MaterialKind::Filament,
            price_per_kg: Some(price_per_kg),
            price_per_liter: None,
            density_g_per_cm3: None,
            efficiency_factor: 1.1,
            color: None,
            vendor: None,
        }
    }

    fn resin_printer() -> PrinterProfile {
        PrinterProfile { name: "sl1-printer".into(), kind: PrinterKind::Resin, wear: bambu_a1().wear }
    }

    /// Like [`filament`], but `kind: Resin` and priced per kg instead of
    /// per liter — used to test the density-conversion fallback.
    fn resin_priced_per_kg(name: &str, price_per_kg: f64) -> FilamentProfile {
        FilamentProfile { kind: MaterialKind::Resin, ..filament(name, price_per_kg) }
    }

    fn base_job(filament: HashMap<String, ProfileRef<FilamentProfile>>, labor_minutes: f64) -> Job {
        Job {
            name: "test".into(),
            source: JobSource::Path("test.gcode.3mf".into()),
            plate_selection: None,
            printer: ProfileRef::Linked { name: "bambu-a1".into() },
            filament,
            material_efficiency_factor_override: None,
            electricity_cost_per_kwh_override: None,
            labor: LaborCost { rate_per_hour: Some(15.0), minutes: labor_minutes },
            hardware: Vec::new(),
            markup_percentages_override: None,
        }
    }

    /// Reference worksheet example: 800 dollar printer, 5yr life, 50%
    /// uptime, 160W, 0.14/kWh, 1.3 buffer.
    #[test]
    fn wear_and_electricity_rate_per_hour() {
        let wear = &bambu_a1().wear;
        let capital_cost_per_hour =
            (wear.purchase_price + wear.annual_maintenance_cost * wear.estimated_life_years)
                / (wear.estimated_life_years * 8760.0 * wear.estimated_uptime_percent);
        let electrical_cost_per_hour = (wear.power_consumption_watts / 1000.0) * 0.14;
        assert!((capital_cost_per_hour - 0.0548).abs() < 1e-3);
        assert!((electrical_cost_per_hour - 0.0224).abs() < 1e-4);
        let combined = (capital_cost_per_hour + electrical_cost_per_hour) * wear.cost_buffer_factor;
        assert!((combined - 0.10).abs() < 5e-3);
    }

    /// Multi-plate worked example: two Bambu plates, single filament.
    #[test]
    fn multi_plate_single_filament_matches_worked_example() {
        let slice = SliceSummary {
            source_format: SourceFormat::Bambu3mf,
            plates: vec![
                PlateSummary {
                    plate_index: 1,
                    print_time_seconds: 1037.0,
                    filament_usage: vec![FilamentUsage {
                        slot_id: SINGLE_FILAMENT_SLOT.into(),
                        material_type: Some("PLA".into()),
                        color: None,
                        grams: Some(6.66),
                        ml: None,
                    }],
                    printer_model: None,
                    layer_height: None,
                    extra: HashMap::new(),
                },
                PlateSummary {
                    plate_index: 2,
                    print_time_seconds: 988.0,
                    filament_usage: vec![FilamentUsage {
                        slot_id: SINGLE_FILAMENT_SLOT.into(),
                        material_type: Some("PLA".into()),
                        color: None,
                        grams: Some(5.15),
                        ml: None,
                    }],
                    printer_model: None,
                    layer_height: None,
                    extra: HashMap::new(),
                },
            ],
        };
        let filament_profiles = HashMap::from([(SINGLE_FILAMENT_SLOT.to_string(), filament("pla-black", 18.99))]);
        let job = base_job(
            HashMap::from([(SINGLE_FILAMENT_SLOT.to_string(), ProfileRef::Linked { name: "pla-black".into() })]),
            10.0,
        );
        let printer = bambu_a1();
        let global = GlobalConfig::default();

        let breakdown = calculate_cost(&job, &slice, &printer, &filament_profiles, &global).unwrap();

        assert_eq!(
            breakdown.plate_breakdown,
            vec![
                PlateLine { plate_index: 1, print_time_seconds: 1037.0, grams: 6.66, ml: 0.0 },
                PlateLine { plate_index: 2, print_time_seconds: 988.0, grams: 5.15, ml: 0.0 },
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
        assert!((line.amount - 12.991).abs() < 1e-9);
        assert!((breakdown.wear_cost - 0.0401).abs() < 1e-3);
        assert!((breakdown.electricity_cost - 0.0164).abs() < 1e-3);
        assert_eq!(breakdown.labor_cost, 2.5);
        assert_eq!(breakdown.hardware_cost, 0.0);
        assert!(breakdown.hardware_cost.is_sign_positive(), "hardware_cost must not be -0.0");
        assert!(breakdown.hardware_breakdown.is_empty());
        assert!((breakdown.total_cost - 2.8031).abs() < 1e-3);
        assert_eq!(breakdown.suggested_prices.len(), 3);
        for sp in &breakdown.suggested_prices {
            let expected = breakdown.total_cost * (1.0 + sp.markup_percent / 100.0);
            assert!((sp.price - expected).abs() < 1e-9);
        }
    }

    /// Multi-filament worked example: one plate, three AMS slots priced
    /// against three different profiles.
    #[test]
    fn multi_slot_filament_splits_material_cost() {
        let slice = SliceSummary {
            source_format: SourceFormat::Bambu3mf,
            plates: vec![PlateSummary {
                plate_index: 1,
                print_time_seconds: 1360.0, // 22m40s
                filament_usage: vec![
                    FilamentUsage {
                        slot_id: "1".into(),
                        material_type: Some("PLA".into()),
                        color: None,
                        grams: Some(6.20),
                        ml: None,
                    },
                    FilamentUsage {
                        slot_id: "2".into(),
                        material_type: Some("PLA".into()),
                        color: None,
                        grams: Some(3.40),
                        ml: None,
                    },
                    FilamentUsage {
                        slot_id: "4".into(),
                        material_type: Some("PETG".into()),
                        color: None,
                        grams: Some(1.85),
                        ml: None,
                    },
                    FilamentUsage {
                        slot_id: "3".into(),
                        material_type: None,
                        color: None,
                        grams: Some(0.0), // unused slot, no profile mapped
                        ml: None,
                    },
                ],
                printer_model: None,
                layer_height: None,
                extra: HashMap::new(),
            }],
        };
        let filament_profiles = HashMap::from([
            ("1".to_string(), filament("pla-black", 18.99)),
            ("2".to_string(), filament("pla-red", 18.99)),
            ("4".to_string(), filament("petg-clear", 22.99)),
        ]);
        let job = base_job(
            HashMap::from([
                ("1".to_string(), ProfileRef::Linked { name: "pla-black".into() }),
                ("2".to_string(), ProfileRef::Linked { name: "pla-red".into() }),
                ("4".to_string(), ProfileRef::Linked { name: "petg-clear".into() }),
            ]),
            15.0,
        );
        let printer = bambu_a1();
        let global = GlobalConfig::default();

        let breakdown = calculate_cost(&job, &slice, &printer, &filament_profiles, &global).unwrap();

        assert!((breakdown.material_cost - 0.2474).abs() < 1e-3);
        assert_eq!(breakdown.labor_cost, 3.75);
        assert!((breakdown.total_cost - 4.0353).abs() < 1e-3);

        // Sorted by name: petg-clear, pla-black, pla-red.
        assert_eq!(breakdown.material_breakdown.len(), 3);
        let names: Vec<&str> =
            breakdown.material_breakdown.iter().map(|l| l.filament_name.as_str()).collect();
        assert_eq!(names, ["petg-clear", "pla-black", "pla-red"]);
        let sum: f64 = breakdown.material_breakdown.iter().map(|l| l.cost).sum();
        assert!((sum - breakdown.material_cost).abs() < 1e-9);
        let pla_black = breakdown.material_breakdown.iter().find(|l| l.filament_name == "pla-black").unwrap();
        assert!((pla_black.cost - 0.12955).abs() < 1e-3);
        assert_eq!(pla_black.unit, "g");
        assert_eq!(pla_black.unit_price, 18.99);
        assert!((pla_black.amount - 6.82).abs() < 1e-9); // 6.20g * 1.1 efficiency factor
    }

    #[test]
    fn unmapped_slot_with_usage_is_an_error() {
        let slice = SliceSummary {
            source_format: SourceFormat::Bambu3mf,
            plates: vec![PlateSummary {
                plate_index: 1,
                print_time_seconds: 100.0,
                filament_usage: vec![FilamentUsage {
                    slot_id: "2".into(),
                    material_type: None,
                    color: None,
                    grams: Some(5.0),
                    ml: None,
                }],
                printer_model: None,
                layer_height: None,
                extra: HashMap::new(),
            }],
        };
        let job = base_job(
            HashMap::from([("1".to_string(), ProfileRef::Linked { name: "pla-black".into() })]),
            1.0,
        );
        let printer = bambu_a1();
        let global = GlobalConfig::default();

        let err = calculate_cost(&job, &slice, &printer, &HashMap::new(), &global).unwrap_err();
        assert_eq!(err, CalcError::MissingFilamentProfile("2".into()));
    }

    #[test]
    fn unknown_plate_selection_is_an_error() {
        let slice = SliceSummary { source_format: SourceFormat::Bambu3mf, plates: vec![] };
        let mut job = base_job(
            HashMap::from([(SINGLE_FILAMENT_SLOT.to_string(), ProfileRef::Linked { name: "pla-black".into() })]),
            1.0,
        );
        job.plate_selection = Some(3);
        let printer = bambu_a1();
        let global = GlobalConfig::default();

        let err = calculate_cost(&job, &slice, &printer, &HashMap::new(), &global).unwrap_err();
        assert_eq!(err, CalcError::PlateNotFound(3));
    }

    fn resin_slice(ml: f64) -> SliceSummary {
        SliceSummary {
            source_format: SourceFormat::PrusaSl1,
            plates: vec![PlateSummary {
                plate_index: 1,
                print_time_seconds: 3600.0,
                filament_usage: vec![FilamentUsage {
                    slot_id: SINGLE_FILAMENT_SLOT.into(),
                    material_type: None,
                    color: None,
                    grams: None,
                    ml: Some(ml),
                }],
                printer_model: None,
                layer_height: None,
                extra: HashMap::new(),
            }],
        }
    }

    /// A ml-reported usage against a profile that only has `price_per_kg`,
    /// but does have `density_g_per_cm3`, should still price via
    /// conversion instead of erroring.
    #[test]
    fn ml_usage_converts_via_density_when_only_price_per_kg_is_set() {
        let slice = resin_slice(10.0);
        let mut profile = resin_priced_per_kg("kg-priced-resin", 40.0);
        profile.density_g_per_cm3 = Some(1.1);
        let filament_profiles = HashMap::from([(SINGLE_FILAMENT_SLOT.to_string(), profile)]);
        let job = base_job(
            HashMap::from([(
                SINGLE_FILAMENT_SLOT.to_string(),
                ProfileRef::Linked { name: "kg-priced-resin".into() },
            )]),
            0.0,
        );
        let printer = resin_printer();
        let global = GlobalConfig::default();

        let breakdown = calculate_cost(&job, &slice, &printer, &filament_profiles, &global).unwrap();

        // 10ml * 1.1g/ml density = 11g, * 1.1 efficiency factor = 12.1g,
        // * $40/1000 per gram = $0.484.
        assert!((breakdown.material_cost - 0.484).abs() < 1e-9);
    }

    /// Same mismatch, but with no `density_g_per_cm3` to bridge the units —
    /// this is exactly what a real user hit: a filament profile with only
    /// `price_per_kg` used against a resin (`.sl1s`, ml-reported) job.
    #[test]
    fn ml_usage_without_density_and_without_price_per_liter_is_an_error() {
        let slice = resin_slice(10.0);
        let filament_profiles = HashMap::from([(
            SINGLE_FILAMENT_SLOT.to_string(),
            resin_priced_per_kg("kg-only-resin", 40.0),
        )]);
        let job = base_job(
            HashMap::from([(
                SINGLE_FILAMENT_SLOT.to_string(),
                ProfileRef::Linked { name: "kg-only-resin".into() },
            )]),
            0.0,
        );
        let printer = resin_printer();
        let global = GlobalConfig::default();

        let err = calculate_cost(&job, &slice, &printer, &filament_profiles, &global).unwrap_err();
        assert_eq!(
            err,
            CalcError::MissingFilamentPrice { name: "kg-only-resin".into(), usage_unit: "ml" }
        );
    }

    /// The exact scenario a real user hit: an FDM printer profile costed
    /// against a resin (`.sl1s`) sliced file.
    #[test]
    fn fdm_printer_against_resin_slice_is_an_error() {
        let slice = resin_slice(10.0);
        let job = base_job(
            HashMap::from([(SINGLE_FILAMENT_SLOT.to_string(), ProfileRef::Linked { name: "pla-basic".into() })]),
            0.0,
        );
        let printer = bambu_a1(); // Fdm, but `slice` is PrusaSl1 (resin)
        let global = GlobalConfig::default();

        let err = calculate_cost(&job, &slice, &printer, &HashMap::new(), &global).unwrap_err();
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
        let slice = resin_slice(10.0);
        let filament_profiles =
            HashMap::from([(SINGLE_FILAMENT_SLOT.to_string(), filament("pla-basic", 20.0))]); // MaterialKind::Filament
        let job = base_job(
            HashMap::from([(SINGLE_FILAMENT_SLOT.to_string(), ProfileRef::Linked { name: "pla-basic".into() })]),
            0.0,
        );
        let printer = resin_printer();
        let global = GlobalConfig::default();

        let err = calculate_cost(&job, &slice, &printer, &filament_profiles, &global).unwrap_err();
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
        let slice = SliceSummary {
            source_format: SourceFormat::Bambu3mf,
            plates: vec![PlateSummary {
                plate_index: 1,
                print_time_seconds: 600.0,
                filament_usage: vec![],
                printer_model: None,
                layer_height: None,
                extra: HashMap::new(),
            }],
        };
        let mut job = base_job(HashMap::new(), 0.0);
        job.hardware = vec![
            HardwareItem { name: "brass insert M3x5".into(), cost: 0.12, quantity: 4 },
            HardwareItem { name: "magnet 6x2mm".into(), cost: 0.08, quantity: 2 },
        ];
        let printer = bambu_a1();
        let global = GlobalConfig::default();

        let breakdown = calculate_cost(&job, &slice, &printer, &HashMap::new(), &global).unwrap();

        assert_eq!(breakdown.hardware_breakdown.len(), 2);
        assert_eq!(
            breakdown.hardware_breakdown[0],
            HardwareLine { name: "brass insert M3x5".into(), quantity: 4, unit_cost: 0.12, cost: 0.48 }
        );
        assert_eq!(
            breakdown.hardware_breakdown[1],
            HardwareLine { name: "magnet 6x2mm".into(), quantity: 2, unit_cost: 0.08, cost: 0.16 }
        );
        assert!((breakdown.hardware_cost - 0.64).abs() < 1e-9);
    }
}
