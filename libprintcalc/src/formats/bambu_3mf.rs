//! Bambu Studio `.gcode.3mf` parser.

use std::collections::HashMap;
use std::io::Cursor;

use xml::attribute::OwnedAttribute;
use xml::reader::{EventReader, XmlEvent};

use super::slice_summary::{FilamentUsage, PlateSummary, SliceMeta, SliceSummary, SourceFormat};
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
        let settings = read_project_settings(bytes).unwrap_or_default();

        let slice_info = zip_read::read_entry_to_string(bytes, "Metadata/slice_info.config")?;
        let mut plates = parse_slice_info(&slice_info, settings.printer_model.as_deref())?;
        settings.describe_slots(&mut plates);

        Ok(SliceSummary {
            source_format: SourceFormat::Bambu3mf,
            meta: SliceMeta {
                printer_profile: settings.printer_profile,
                print_profile: settings.print_profile,
                printer_variant: settings.printer_variant,
                nozzle_diameter: settings.nozzle_diameter,
                slicer: read_slicer_version(&slice_info),
            },
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

/// Whatever `Metadata/project_settings.config` said about the setup: the
/// printer and profile names, and the per-tray filament presets.
///
/// Bambu Studio stores the filament keys as arrays indexed by AMS tray
/// *position*, i.e. `slice_info.config`'s `<filament id="N">` minus one,
/// so `filament_settings_id[0]` describes tray 1. Everything is
/// best-effort — a missing or unparseable settings file just leaves the
/// summary as bare as it used to be, since none of this is needed to
/// cost a job.
#[derive(Default)]
struct ProjectSettings {
    printer_model: Option<String>,
    printer_profile: Option<String>,
    print_profile: Option<String>,
    printer_variant: Option<String>,
    nozzle_diameter: Option<f64>,
    filament_profiles: Vec<String>,
    filament_vendors: Vec<String>,
    filament_prices: Vec<String>,
    filament_densities: Vec<String>,
}

impl ProjectSettings {
    /// Fills in each plate's `FilamentUsage` from the tray preset behind
    /// its slot id. A slot id that isn't a 1-based tray number (or points
    /// past the end of the arrays) is simply left as the slice_info
    /// parser produced it.
    fn describe_slots(&self, plates: &mut [PlateSummary]) {
        for usage in plates.iter_mut().flat_map(|p| &mut p.filament_usage) {
            let Some(i) = usage
                .slot_id
                .parse::<usize>()
                .ok()
                .and_then(|tray| tray.checked_sub(1))
            else {
                continue;
            };
            usage.profile_name = pick(&self.filament_profiles, i);
            usage.vendor = pick(&self.filament_vendors, i);
            usage.price = pick(&self.filament_prices, i).and_then(|s| s.parse().ok());
            usage.density = pick(&self.filament_densities, i).and_then(|s| s.parse().ok());
        }
    }
}

fn pick(list: &[String], i: usize) -> Option<String> {
    list.get(i).filter(|s| !s.is_empty()).cloned()
}

/// `project_settings.config` is JSON where nearly every value is either a
/// string or an array of strings; these two helpers read whichever shape
/// a key happens to use.
fn json_str(value: &serde_json::Value, key: &str) -> Option<String> {
    match value.get(key)? {
        serde_json::Value::String(s) if !s.is_empty() => Some(s.clone()),
        serde_json::Value::Array(items) => items
            .first()
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(String::from),
        _ => None,
    }
}

fn json_str_array(value: &serde_json::Value, key: &str) -> Vec<String> {
    match value.get(key) {
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .map(|v| v.as_str().unwrap_or_default().to_string())
            .collect(),
        Some(serde_json::Value::String(s)) => vec![s.clone()],
        _ => Vec::new(),
    }
}

/// Best-effort read of the project-wide settings. `None` rather than an
/// error if anything about this is missing or unparseable — it's all
/// nice-to-have labelling, not required for costing.
fn read_project_settings(bytes: &[u8]) -> Option<ProjectSettings> {
    let text = zip_read::read_entry_to_string(bytes, "Metadata/project_settings.config").ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    Some(ProjectSettings {
        printer_model: json_str(&value, "printer_model"),
        printer_profile: json_str(&value, "printer_settings_id"),
        print_profile: json_str(&value, "print_settings_id"),
        printer_variant: json_str(&value, "printer_variant"),
        nozzle_diameter: json_str(&value, "nozzle_diameter").and_then(|s| s.parse().ok()),
        filament_profiles: json_str_array(&value, "filament_settings_id"),
        filament_vendors: json_str_array(&value, "filament_vendor"),
        filament_prices: json_str_array(&value, "filament_cost"),
        filament_densities: json_str_array(&value, "filament_density"),
    })
}

/// `"BambuStudio 02.07.01.62"` from `slice_info.config`'s `<header>`, if
/// it names a slicer client and version.
fn read_slicer_version(slice_info: &str) -> Option<String> {
    let version = slice_info
        .split("X-BBL-Client-Version")
        .nth(1)?
        .split("value=\"")
        .nth(1)?
        .split('"')
        .next()?;
    (!version.is_empty()).then(|| format!("BambuStudio {version}"))
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
                        meters: attr(&attributes, "used_m").and_then(|v| v.parse().ok()),
                        ..FilamentUsage::default()
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

    /// `project_settings.config` names the printer profile, and one
    /// filament preset per AMS tray — tray N describes slot id "N".
    #[test]
    fn reads_project_settings_into_meta_and_slots() {
        let bytes = read_test_file("single_plate_multicolor.gcode.3mf");
        let summary = Bambu3mf::parse(&bytes).unwrap();

        assert_eq!(
            summary.meta.printer_profile.as_deref(),
            Some("Bambu Lab P2S 0.4 nozzle")
        );
        assert_eq!(
            summary.meta.print_profile.as_deref(),
            Some("0.24mm Standard @BBL P2S")
        );
        assert_eq!(summary.meta.nozzle_diameter, Some(0.4));
        assert_eq!(
            summary.meta.slicer.as_deref(),
            Some("BambuStudio 02.07.01.62")
        );

        let usage = &summary.plates[0].filament_usage;
        assert_eq!(usage.len(), 2);
        for (i, u) in usage.iter().enumerate() {
            assert_eq!(u.slot_id, (i + 1).to_string());
            assert_eq!(u.profile_name.as_deref(), Some("Bambu PLA Basic @BBL P2S"));
            assert_eq!(u.vendor.as_deref(), Some("Bambu Lab"));
            assert_eq!(u.price, Some(24.99));
            assert_eq!(u.density, Some(1.26));
        }
        // Distinct per tray even though both trays hold the same preset.
        assert_eq!(usage[0].color.as_deref(), Some("#AE6342"));
        assert_eq!(usage[1].color.as_deref(), Some("#F8F4EF"));
        assert_eq!(usage[0].meters, Some(1.60));
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
