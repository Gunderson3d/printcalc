//! Bambu Studio `.gcode.3mf` parser.


use std::collections::HashMap;
use std::io::Cursor;

use xml::attribute::OwnedAttribute;
use xml::reader::{EventReader, XmlEvent};

use super::slice_summary::{FilamentUsage, PlateSummary, SliceSummary, SourceFormat};
use super::{ParseError, SliceParser, zip_read};

/// [`SliceParser`] for Bambu Studio `.gcode.3mf` — see the module docs.
pub struct Bambu3mf;

impl SliceParser for Bambu3mf {
    fn detect(bytes: &[u8], filename: &str) -> bool {
        if filename.to_ascii_lowercase().ends_with(".3mf") {
            return true;
        }
        looks_like_3mf_zip(bytes)
    }

    fn parse(bytes: &[u8]) -> Result<SliceSummary, ParseError> {
        let printer_model = read_printer_model(bytes);

        let slice_info = zip_read::read_entry_to_string(bytes, "Metadata/slice_info.config")?;
        let plates = parse_slice_info(&slice_info, printer_model.as_deref())?;

        Ok(SliceSummary {
            source_format: SourceFormat::Bambu3mf,
            plates,
        })
    }
}

/// Content-sniff fallback for a renamed file: a zip containing
/// `3D/3dmodel.model` — the one entry every 3MF has that a Prusa `.sl1`/
/// `.sl1s` zip (which has `config.ini` instead) never does.
fn looks_like_3mf_zip(bytes: &[u8]) -> bool {
    zip_read::has_entry(bytes, "3D/3dmodel.model")
}

/// Best-effort: a human-readable printer name from the project-wide
/// settings (e.g. "Bambu Lab P2S"). `None` rather than an error if
/// anything about this is missing or unparseable — it's a nice-to-have
/// label, not required for costing.
fn read_printer_model(bytes: &[u8]) -> Option<String> {
    let text = zip_read::read_entry_to_string(bytes, "Metadata/project_settings.config").ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    value.get("printer_model")?.as_str().map(String::from)
}

fn attr(attributes: &[OwnedAttribute], name: &str) -> Option<String> {
    attributes
        .iter()
        .find(|a| a.name.local_name == name)
        .map(|a| a.value.clone())
}

/// Walks `<config><plate>...</plate>...</config>`, one [`PlateSummary`] per
/// `<plate>`, one [`FilamentUsage`] per `<filament>` inside it (slot_id =
/// its `id` attribute, grams = `used_g`). `<metadata key="weight">` is the
/// slicer's own total and isn't separately stored — it's just the sum of
/// the `<filament>` elements already captured.
fn parse_slice_info(
    xml_text: &str,
    printer_model: Option<&str>,
) -> Result<Vec<PlateSummary>, ParseError> {
    let mut plates = Vec::new();
    let mut in_plate = false;
    let mut plate_index: Option<u32> = None;
    let mut print_time_seconds: Option<f64> = None;
    let mut filament_usage: Vec<FilamentUsage> = Vec::new();

    for event in EventReader::new(Cursor::new(xml_text.as_bytes())) {
        let event = event.map_err(|_| {
            ParseError::InvalidField("slice_info.config", "malformed XML".to_string())
        })?;
        match event {
            XmlEvent::StartElement {
                name, attributes, ..
            } => match name.local_name.as_str() {
                "plate" => {
                    in_plate = true;
                    plate_index = None;
                    print_time_seconds = None;
                    filament_usage = Vec::new();
                }
                "metadata" if in_plate => match attr(&attributes, "key").as_deref() {
                    Some("index") => {
                        plate_index = attr(&attributes, "value").and_then(|v| v.parse().ok())
                    }
                    Some("prediction") => {
                        print_time_seconds = attr(&attributes, "value").and_then(|v| v.parse().ok())
                    }
                    _ => {}
                },
                "filament" if in_plate => {
                    filament_usage.push(FilamentUsage {
                        slot_id: attr(&attributes, "id").unwrap_or_default(),
                        material_type: attr(&attributes, "type"),
                        color: attr(&attributes, "color"),
                        grams: attr(&attributes, "used_g").and_then(|v| v.parse().ok()),
                        ml: None,
                    });
                }
                _ => {}
            },
            XmlEvent::EndElement { name } if name.local_name == "plate" => {
                in_plate = false;
                let index = plate_index
                    .ok_or(ParseError::MissingField("slice_info.config: plate index"))?;
                let seconds = print_time_seconds.ok_or(ParseError::MissingField(
                    "slice_info.config: plate prediction",
                ))?;
                plates.push(PlateSummary {
                    plate_index: index,
                    print_time_seconds: seconds,
                    filament_usage: std::mem::take(&mut filament_usage),
                    printer_model: printer_model.map(String::from),
                    layer_height: None,
                    extra: HashMap::new(),
                });
            }
            _ => {}
        }
    }

    if plates.is_empty() {
        return Err(ParseError::MissingField(
            "slice_info.config: no <plate> elements",
        ));
    }
    Ok(plates)
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
        assert!(Bambu3mf::detect(b"", "widget.gcode.3mf"));
        assert!(!Bambu3mf::detect(b"PK\x03\x04not a 3mf", "widget.sl1"));
    }

    #[test]
    fn parses_single_plate() {
        let bytes = read_test_file("single_plate.gcode.3mf");
        assert!(Bambu3mf::detect(&bytes, "single_plate.gcode.3mf"));

        let summary = Bambu3mf::parse(&bytes).unwrap();
        assert_eq!(summary.source_format, SourceFormat::Bambu3mf);
        assert_eq!(summary.plates.len(), 1);
        let plate = &summary.plates[0];
        assert_eq!(plate.plate_index, 1);
        assert_eq!(plate.print_time_seconds, 1037.0);
        assert_eq!(plate.printer_model.as_deref(), Some("Bambu Lab P2S"));
        assert_eq!(plate.filament_usage.len(), 1);
        let usage = &plate.filament_usage[0];
        assert_eq!(usage.slot_id, "1");
        assert_eq!(usage.material_type.as_deref(), Some("PLA"));
        assert_eq!(usage.color.as_deref(), Some("#AE6342"));
        assert_eq!(usage.grams, Some(6.66));
        assert_eq!(usage.ml, None);
    }

    #[test]
    fn parses_multi_plate() {
        let bytes = read_test_file("multi_plate.gcode.3mf");
        let summary = Bambu3mf::parse(&bytes).unwrap();
        assert_eq!(summary.plates.len(), 2);
        assert_eq!(summary.plates[0].plate_index, 1);
        assert_eq!(summary.plates[0].print_time_seconds, 1037.0);
        assert_eq!(summary.plates[0].filament_usage[0].grams, Some(6.66));
        assert_eq!(summary.plates[1].plate_index, 2);
        assert_eq!(summary.plates[1].print_time_seconds, 988.0);
        assert_eq!(summary.plates[1].filament_usage[0].grams, Some(5.15));
    }

    #[test]
    fn parses_single_plate_multicolor() {
        let bytes = read_test_file("single_plate_multicolor.gcode.3mf");
        let summary = Bambu3mf::parse(&bytes).unwrap();
        assert_eq!(summary.plates.len(), 1);
        let usage = &summary.plates[0].filament_usage;
        assert_eq!(usage.len(), 2);
        assert_eq!(usage[0].slot_id, "1");
        assert_eq!(usage[0].grams, Some(4.83));
        assert_eq!(usage[1].slot_id, "2");
        assert_eq!(usage[1].grams, Some(3.87));
    }

    #[test]
    fn parses_multi_plate_multicolor() {
        let bytes = read_test_file("multi_plate_multicolor.gcode.3mf");
        let summary = Bambu3mf::parse(&bytes).unwrap();
        assert_eq!(summary.plates.len(), 2);
        for plate in &summary.plates {
            assert_eq!(plate.filament_usage.len(), 2);
        }
    }
}
