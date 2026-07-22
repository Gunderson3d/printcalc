//! `CostBreakdown` and `SuggestedPrice`. See `SPEC.md` §2, §3.

use std::collections::HashMap;
use std::fmt;

use serde::Serialize;

use super::filament::FilamentProfile;
use super::job::Job;
use super::printer::PrinterProfile;
use crate::formats::slice_summary::{PlateSummary, SliceSummary};
use crate::library::GlobalConfig;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CostBreakdown {
    pub material_cost: f64,
    pub wear_cost: f64,
    pub electricity_cost: f64,
    pub labor_cost: f64,
    pub hardware_cost: f64,
    pub total_cost: f64,
    pub suggested_prices: Vec<SuggestedPrice>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct SuggestedPrice {
    pub markup_percent: f64,
    pub price: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CalcError {
    /// `job.plate_selection` named a plate that isn't in the sliced file.
    PlateNotFound(u32),
    /// A `FilamentUsage` slot had nonzero usage but no entry in
    /// `job.filament`.
    MissingFilamentProfile(String),
    /// A filament profile had neither `price_per_kg` nor `price_per_liter`
    /// set.
    MissingFilamentPrice(String),
}

impl fmt::Display for CalcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CalcError::PlateNotFound(i) => write!(f, "plate {i} not found in sliced file"),
            CalcError::MissingFilamentProfile(slot) => {
                write!(f, "no filament profile mapped for slot {slot}")
            }
            CalcError::MissingFilamentPrice(name) => {
                write!(f, "filament profile {name:?} has no price_per_kg or price_per_liter")
            }
        }
    }
}

impl std::error::Error for CalcError {}

/// Cost breakdown for `job` against the already-parsed `slice`, using
/// already-resolved profiles (a [`super::job::ProfileRef::Linked`] must be
/// resolved against the profile library — or, for WASM, supplied directly
/// by the caller — before reaching here; see `SPEC.md` §3, §6).
pub fn calculate_cost(
    job: &Job,
    slice: &SliceSummary,
    printer: &PrinterProfile,
    filament_profiles: &HashMap<String, FilamentProfile>,
    global: &GlobalConfig,
) -> Result<CostBreakdown, CalcError> {
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

    let mut material_cost = 0.0;
    for plate in &selected_plates {
        for usage in &plate.filament_usage {
            // grams+price_per_kg or ml+price_per_liter, whichever the usage
            // entry actually carries; a slot with zero of both is present
            // but unused (Bambu AMS tray / Prusa extruder not loaded) and
            // is skipped, per SPEC.md §3/§4.
            let (amount, want_grams) =
                match (usage.grams.filter(|&g| g != 0.0), usage.ml.filter(|&m| m != 0.0)) {
                    (Some(g), _) => (g, true),
                    (None, Some(ml)) => (ml, false),
                    (None, None) => continue,
                };
            let profile = filament_profiles
                .get(&usage.slot_id)
                .ok_or_else(|| CalcError::MissingFilamentProfile(usage.slot_id.clone()))?;
            let price_per_unit = if want_grams { profile.price_per_kg } else { profile.price_per_liter }
                .ok_or_else(|| CalcError::MissingFilamentPrice(profile.name.clone()))?;
            let effective_factor = job
                .material_efficiency_factor_override
                .unwrap_or(profile.efficiency_factor);
            material_cost += amount * effective_factor * (price_per_unit / 1000.0);
        }
    }

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

    let hardware_cost: f64 = job.hardware.iter().map(|h| h.cost * h.quantity as f64).sum();

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
        material_cost,
        wear_cost,
        electricity_cost,
        labor_cost,
        hardware_cost,
        total_cost,
        suggested_prices,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::slice_summary::{FilamentUsage, PlateSummary, SourceFormat};
    use crate::model::filament::MaterialKind;
    use crate::model::job::{JobSource, LaborCost, ProfileRef, SINGLE_FILAMENT_SLOT};
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

    /// Reference worksheet example from SPEC.md §3: 800 dollar printer, 5yr
    /// life, 50% uptime, 160W, 0.14/kWh, 1.3 buffer.
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

    /// SPEC.md §7 multi-plate example: two Bambu plates, single filament.
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

        assert!((breakdown.material_cost - 0.2467).abs() < 1e-3);
        assert!((breakdown.wear_cost - 0.0401).abs() < 1e-3);
        assert!((breakdown.electricity_cost - 0.0164).abs() < 1e-3);
        assert_eq!(breakdown.labor_cost, 2.5);
        assert_eq!(breakdown.hardware_cost, 0.0);
        assert!((breakdown.total_cost - 2.8031).abs() < 1e-3);
        assert_eq!(breakdown.suggested_prices.len(), 3);
        for sp in &breakdown.suggested_prices {
            let expected = breakdown.total_cost * (1.0 + sp.markup_percent / 100.0);
            assert!((sp.price - expected).abs() < 1e-9);
        }
    }

    /// SPEC.md §7 multi-filament example: one plate, three AMS slots priced
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
}
