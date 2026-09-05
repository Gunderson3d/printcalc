//! PrusaSlicer resin `.sl1` / `.sl1s` parser.

use std::collections::HashMap;

use super::slice_summary::{FilamentUsage, PlateSummary, SliceMeta, SliceSummary, SourceFormat};
use super::{ParseError, SliceParser, zip_read};

/// [`SliceParser`] for PrusaSlicer resin `.sl1`/`.sl1s` — see the module docs.
pub struct PrusaSl1;

impl SliceParser for PrusaSl1 {
    fn detect(bytes: &[u8], filename: &str) -> bool {
        let lower = filename.to_ascii_lowercase();
        if lower.ends_with(".sl1") || lower.ends_with(".sl1s") {
            return true;
        }
        looks_like_sl1_zip(bytes)
    }

    fn parse(bytes: &[u8]) -> Result<SliceSummary, ParseError> {
        let config = read_config_ini(bytes)?;
        // `prusaslicer.ini` carries the full profile PrusaSlicer sliced
        // with — resin vendor, colour, bottle price. It's best-effort:
        // `config.ini` alone is enough to cost the job.
        let profile = read_ini_entry(bytes, "prusaslicer.ini").unwrap_or_default();

        let print_time_seconds = required_f64(&config, "printTime")?;
        let used_material_ml = required_f64(&config, "usedMaterial")?;
        let layer_height = config
            .get("layerHeight")
            .and_then(|v| v.parse::<f64>().ok());

        Ok(SliceSummary {
            source_format: SourceFormat::PrusaSl1,
            meta: SliceMeta {
                printer_profile: config
                    .get("printerProfile")
                    .or_else(|| profile.get("printer_settings_id"))
                    .cloned(),
                print_profile: config
                    .get("printProfile")
                    .or_else(|| profile.get("sla_print_settings_id"))
                    .cloned(),
                printer_variant: config.get("printerVariant").cloned(),
                nozzle_diameter: None, // resin: no nozzle
                slicer: config.get("prusaSlicerVersion").cloned(),
            },
            plates: vec![PlateSummary {
                plate_index: 1,
                print_time_seconds,
                filament_usage: vec![FilamentUsage {
                    slot_id: "1".to_string(),
                    material_type: profile
                        .get("material_type")
                        .cloned()
                        .or_else(|| config.get("materialName").cloned()),
                    color: profile.get("material_colour").cloned(),
                    profile_name: config
                        .get("materialName")
                        .or_else(|| profile.get("sla_material_settings_id"))
                        .cloned(),
                    vendor: profile.get("material_vendor").cloned(),
                    // `bottle_cost`/`bottle_volume` are per-bottle; the
                    // cost model prices resin per liter, same basis as
                    // filament's per-kg. A zero-cost bottle (the
                    // PrusaSlicer default) means "unpriced", not free.
                    price: bottle_price_per_liter(&profile),
                    density: profile
                        .get("material_density")
                        .and_then(|v| v.parse::<f64>().ok()),
                    meters: None,
                    grams: None,
                    ml: Some(used_material_ml),
                }],
                printer_model: config.get("printerModel").cloned(),
                layer_height,
                extra: HashMap::new(),
            }],
        })
    }
}

/// `bottle_cost` / `bottle_volume` (ml) scaled to a per-liter price, so it
/// lines up with [`crate::MaterialProfile::price`]'s per-liter basis for
/// resin. `None` when either is missing, unparseable, or zero.
fn bottle_price_per_liter(profile: &HashMap<String, String>) -> Option<f64> {
    let cost: f64 = profile.get("bottle_cost")?.parse().ok()?;
    let volume_ml: f64 = profile.get("bottle_volume")?.parse().ok()?;
    (cost > 0.0 && volume_ml > 0.0).then(|| cost * 1000.0 / volume_ml)
}

fn looks_like_sl1_zip(bytes: &[u8]) -> bool {
    zip_read::has_entry(bytes, "config.ini")
}

fn read_config_ini(bytes: &[u8]) -> Result<HashMap<String, String>, ParseError> {
    let text = zip_read::read_entry_to_string(bytes, "config.ini")?;
    Ok(parse_ini(&text))
}

/// Same as [`read_config_ini`] for an optional entry — a missing or
/// unreadable file is just an empty map, never an error.
fn read_ini_entry(bytes: &[u8], entry: &str) -> Option<HashMap<String, String>> {
    zip_read::read_entry_to_string(bytes, entry)
        .ok()
        .map(|text| parse_ini(&text))
}

fn parse_ini(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            let value = value.trim();
            (!value.is_empty()).then(|| (key.trim().to_string(), value.to_string()))
        })
        .collect()
}

fn required_f64(config: &HashMap<String, String>, key: &'static str) -> Result<f64, ParseError> {
    let raw = config.get(key).ok_or(ParseError::MissingField(key))?;
    raw.parse::<f64>()
        .map_err(|_| ParseError::InvalidField(key, raw.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_test_file(name: &str) -> Vec<u8> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../Testfiles")
            .join(name);
        std::fs::read(path).expect("test fixture missing")
    }

    #[test]
    fn detects_by_extension() {
        assert!(PrusaSl1::detect(b"", "Shape-Box.sl1"));
        assert!(PrusaSl1::detect(b"", "Shape-Box.sl1s"));
        assert!(!PrusaSl1::detect(b"", "widget.gcode"));
    }

    #[test]
    fn parses_sl1s_fixture() {
        let bytes = read_test_file("Shape-Box.sl1s");
        assert!(PrusaSl1::detect(&bytes, "Shape-Box.sl1s"));

        let summary = PrusaSl1::parse(&bytes).unwrap();
        assert_eq!(summary.source_format, SourceFormat::PrusaSl1);
        assert_eq!(summary.plates.len(), 1);
        let plate = &summary.plates[0];
        assert_eq!(plate.plate_index, 1);
        assert!((plate.print_time_seconds - 2886.405).abs() < 1e-6);
        assert_eq!(plate.printer_model.as_deref(), Some("SL1S"));
        assert_eq!(plate.layer_height, Some(0.05));
        assert_eq!(plate.filament_usage.len(), 1);
        let usage = &plate.filament_usage[0];
        assert_eq!(usage.slot_id, "1");
        assert!((usage.ml.unwrap() - 2.598598).abs() < 1e-6);
        assert_eq!(usage.grams, None);
        // `material_type` is the family, as it is for FDM ("PLA"); the
        // full preset name lives in `profile_name`.
        assert_eq!(usage.material_type.as_deref(), Some("Model"));
        assert_eq!(
            usage.profile_name.as_deref(),
            Some("Prusament Resin Model+ Prusa Orange @0.05 SL1S")
        );
        assert_eq!(usage.vendor.as_deref(), Some("Prusa Polymers"));
        assert_eq!(usage.color.as_deref(), Some("#FF8040"));
        assert_eq!(usage.density, Some(1.1));
        // bottle_cost is 0 in this fixture — unpriced, not free.
        assert_eq!(usage.price, None);
        assert_eq!(
            summary.meta.printer_profile.as_deref(),
            Some("Original Prusa SL1S SPEED")
        );
        assert_eq!(
            summary.meta.print_profile.as_deref(),
            Some("0.05 Normal @SL1S")
        );
        assert_eq!(
            summary.meta.slicer.as_deref(),
            Some("PrusaSlicer-2.9.6+flathub.org")
        );
    }

    #[test]
    fn parses_sl1_fixture() {
        let bytes = read_test_file("Shape-Box.sl1");
        let summary = PrusaSl1::parse(&bytes).unwrap();
        let plate = &summary.plates[0];
        assert!((plate.print_time_seconds - 4046.414).abs() < 1e-6);
        assert!((plate.filament_usage[0].ml.unwrap() - 2.457665).abs() < 1e-6);
        assert_eq!(plate.printer_model.as_deref(), Some("SL1"));
    }
}
