//! PrusaSlicer binary G-code (`.bgcode`) parser — the newer default output
//! format for MK4/MK4S/MINI+/XL firmware.
//!
//! Binary block container, magic `GCDE`. Confirmed against
//! `Testfiles/Shape-Box_0.4n_0.2mm_PLA_MINIIS_24m.bgcode`: the
//! `PrinterMetadata` block holds the exact same `key=value` text schema as
//! the plain `.gcode` footer (`filament used [g]=`, `estimated printing
//! time (normal mode)=`, `printer_model=`, `filament_type=`,
//! `layer_height=`, ...), just without the `; ` comment prefix or spaces
//! around `=` — `prusa_metadata::parse_lines` (private) handles both shapes
//! already (it only trims and splits on `=`). See `prusa_metadata` (private)
//! for the shared key=value parsing used by both this and
//! [`super::prusa_gcode`].

use binarygcode::{BlockKind, DeserialisedResult, Deserialiser};

use super::slice_summary::{SliceSummary, SourceFormat};
use super::{ParseError, SliceParser, prusa_metadata};

/// [`SliceParser`] for PrusaSlicer binary `.bgcode` — see the module docs.
pub struct PrusaBgcode;

impl SliceParser for PrusaBgcode {
    fn detect(bytes: &[u8], filename: &str) -> bool {
        if filename.to_ascii_lowercase().ends_with(".bgcode") {
            return true;
        }
        bytes.starts_with(b"GCDE")
    }

    fn parse(bytes: &[u8]) -> Result<SliceSummary, ParseError> {
        let text = metadata_text(bytes)?;
        let metadata = prusa_metadata::parse_lines(text.lines());
        prusa_metadata::build_slice_summary(&metadata, SourceFormat::PrusaBgcode)
    }
}

/// Decodes just the metadata blocks (file/printer/print/slicer) to ASCII
/// `key=value` text, skipping the `GCode` block (the actual, often large,
/// compressed motion stream) and any `Thumbnail` blocks (embedded preview
/// images) entirely — we only ever need the metadata.
fn metadata_text(bytes: &[u8]) -> Result<String, ParseError> {
    if !bytes.starts_with(b"GCDE") {
        return Err(ParseError::NotThisFormat);
    }

    let mut deserialiser = Deserialiser::default();
    deserialiser.digest(bytes);

    let mut buf = Vec::new();
    loop {
        match deserialiser
            .deserialise()
            .map_err(|e| ParseError::InvalidField("bgcode", e.to_string()))?
        {
            DeserialisedResult::FileHeader(_) => {}
            DeserialisedResult::Block(mut block) => {
                if !matches!(block.kind, BlockKind::GCode | BlockKind::Thumbnail) {
                    block
                        .to_ascii(&mut buf, false)
                        .map_err(|e| ParseError::InvalidField("bgcode", e.to_string()))?;
                }
            }
            DeserialisedResult::MoreBytesRequired(_) => break,
        }
    }
    String::from_utf8(buf).map_err(|e| ParseError::InvalidField("bgcode", e.to_string()))
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
    fn detects_by_extension_and_magic() {
        assert!(PrusaBgcode::detect(b"", "widget.bgcode"));
        assert!(PrusaBgcode::detect(b"GCDEanything", "widget"));
        assert!(!PrusaBgcode::detect(b"not gcde", "widget.gcode"));
    }

    #[test]
    fn parses_real_fixture() {
        let bytes = read_test_file("Shape-Box_0.4n_0.2mm_PLA_MINIIS_24m.bgcode");
        assert!(PrusaBgcode::detect(
            &bytes,
            "Shape-Box_0.4n_0.2mm_PLA_MINIIS_24m.bgcode"
        ));

        let summary = PrusaBgcode::parse(&bytes).unwrap();
        assert_eq!(summary.source_format, SourceFormat::PrusaBgcode);
        assert_eq!(summary.plates.len(), 1);
        let plate = &summary.plates[0];
        // "23m 57s"
        assert!((plate.print_time_seconds - 1437.0).abs() < 1e-9);
        assert_eq!(plate.printer_model.as_deref(), Some("MINIIS"));
        assert_eq!(plate.layer_height, Some(0.2));
        assert_eq!(plate.filament_usage.len(), 1);
        let usage = &plate.filament_usage[0];
        assert_eq!(usage.slot_id, "1");
        assert!((usage.grams.unwrap() - 2.90).abs() < 1e-9);
        assert_eq!(usage.material_type.as_deref(), Some("PLA"));
    }
}
