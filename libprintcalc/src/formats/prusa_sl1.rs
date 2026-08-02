//! PrusaSlicer resin `.sl1` / `.sl1s` parser.

use std::collections::HashMap;

use super::slice_summary::{FilamentUsage, PlateSummary, SliceSummary, SourceFormat};
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

        let print_time_seconds = required_f64(&config, "printTime")?;
        let used_material_ml = required_f64(&config, "usedMaterial")?;
        let layer_height = config
            .get("layerHeight")
            .and_then(|v| v.parse::<f64>().ok());

        Ok(SliceSummary {
            source_format: SourceFormat::PrusaSl1,
            plates: vec![PlateSummary {
                plate_index: 1,
                print_time_seconds,
                filament_usage: vec![FilamentUsage {
                    slot_id: "1".to_string(),
                    material_type: config.get("materialName").cloned(),
                    color: None,
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

fn looks_like_sl1_zip(bytes: &[u8]) -> bool {
    zip_read::has_entry(bytes, "config.ini")
}

fn read_config_ini(bytes: &[u8]) -> Result<HashMap<String, String>, ParseError> {
    let text = zip_read::read_entry_to_string(bytes, "config.ini")?;
    Ok(text
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            Some((key.trim().to_string(), value.trim().to_string()))
        })
        .collect())
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
        assert_eq!(
            usage.material_type.as_deref(),
            Some("Prusament Resin Model+ Prusa Orange @0.05 SL1S")
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
