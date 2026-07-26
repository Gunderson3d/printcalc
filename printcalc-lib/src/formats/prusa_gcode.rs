//! PrusaSlicer FDM `.gcode` parser.

use super::slice_summary::{SliceSummary, SourceFormat};
use super::{ParseError, SliceParser, prusa_metadata};

/// [`SliceParser`] for PrusaSlicer/OrcaSlicer plain-text `.gcode` — see
/// the module docs.
pub struct PrusaGcode;

impl SliceParser for PrusaGcode {
    fn detect(bytes: &[u8], filename: &str) -> bool {
        if filename.to_ascii_lowercase().ends_with(".gcode") {
            return true;
        }
        looks_like_plain_gcode(bytes)
    }

    fn parse(bytes: &[u8]) -> Result<SliceSummary, ParseError> {
        let text = std::str::from_utf8(bytes).map_err(|_| ParseError::NotThisFormat)?;
        let comment_lines = text
            .lines()
            .filter_map(|line| line.trim_start().strip_prefix(';'));
        let metadata = prusa_metadata::parse_lines(comment_lines);
        prusa_metadata::build_slice_summary(&metadata, SourceFormat::PrusaGcode)
    }
}

/// Content-sniff fallback for a renamed file: not a zip (3MF/sl1), not a
/// `.bgcode` (`GCDE` magic), and decodes as UTF-8 text.
fn looks_like_plain_gcode(bytes: &[u8]) -> bool {
    !bytes.starts_with(b"PK\x03\x04")
        && !bytes.starts_with(b"GCDE")
        && std::str::from_utf8(bytes).is_ok()
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
        assert!(PrusaGcode::detect(b"", "widget.gcode"));
        // A renamed zip (3MF/sl1/sl1s) must not be picked up by the
        // plain-text fallback just because its extension doesn't match.
        assert!(!PrusaGcode::detect(
            b"PK\x03\x04not actually gcode",
            "widget.sl1"
        ));
        assert!(!PrusaGcode::detect(
            b"GCDE not actually gcode either",
            "widget.bgcode"
        ));
    }

    #[test]
    fn parses_real_fixture() {
        let bytes = read_test_file("Cube_0.2mm_PLA_Prusa MINI_20m7s.gcode");
        assert!(PrusaGcode::detect(
            &bytes,
            "Cube_0.2mm_PLA_Prusa MINI_20m7s.gcode"
        ));

        let summary = PrusaGcode::parse(&bytes).unwrap();
        assert_eq!(summary.source_format, SourceFormat::PrusaGcode);
        assert_eq!(summary.plates.len(), 1);
        let plate = &summary.plates[0];
        assert_eq!(plate.plate_index, 1);
        // "20m 7s"
        assert!((plate.print_time_seconds - 1207.0).abs() < 1e-9);
        assert_eq!(plate.printer_model.as_deref(), Some("Prusa MINI"));
        assert_eq!(plate.layer_height, Some(0.2));
        assert_eq!(plate.filament_usage.len(), 1);
        let usage = &plate.filament_usage[0];
        assert_eq!(usage.slot_id, "1");
        assert!((usage.grams.unwrap() - 3.10).abs() < 1e-9);
        assert_eq!(usage.ml, None);
        assert_eq!(usage.material_type.as_deref(), Some("PLA"));
    }

    /// Core One IDEX, sliced single-extruder: `filament used [g]` is still
    /// comma-shaped (`"2.82, 0.00, ..."`, 8 slots — the printer's capable
    /// of more, unused ones are 0.00), so this is the milestone-6
    /// comma-list path even though only one slot ends up nonzero. slot_id
    /// is therefore the 0-based list position ("0"), not the scalar-case
    /// "1" — a real, intended asymmetry.
    #[test]
    fn parses_core_one_idex_single_extruder_fixture() {
        let bytes = read_test_file("Shape-Box_0.4n_0.2mm_PLA_COREONEINDX_19m.gcode");
        let summary = PrusaGcode::parse(&bytes).unwrap();
        let plate = &summary.plates[0];
        assert_eq!(plate.printer_model.as_deref(), Some("COREONE_INDX8T"));
        assert_eq!(plate.filament_usage.len(), 1);
        let usage = &plate.filament_usage[0];
        assert_eq!(usage.slot_id, "0");
        assert!((usage.grams.unwrap() - 2.82).abs() < 1e-9);
        assert_eq!(usage.material_type.as_deref(), Some("PLA"));
        assert_eq!(usage.color.as_deref(), Some("#FF8000"));
    }

    /// Same model, dual-extruder slice of the same part: two nonzero
    /// entries, slot_id "0" and "1".
    #[test]
    fn parses_core_one_idex_dual_extruder_fixture() {
        let bytes = read_test_file("Shape-Box_0.4n_0.2mm_PLA,PLA_COREONEINDX_19m.gcode");
        let summary = PrusaGcode::parse(&bytes).unwrap();
        let plate = &summary.plates[0];
        assert_eq!(plate.filament_usage.len(), 2);
        assert_eq!(plate.filament_usage[0].slot_id, "0");
        assert!((plate.filament_usage[0].grams.unwrap() - 1.38).abs() < 1e-9);
        assert_eq!(plate.filament_usage[1].slot_id, "1");
        assert!((plate.filament_usage[1].grams.unwrap() - 1.47).abs() < 1e-9);
    }
}
