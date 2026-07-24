//! Shared `key = value` text metadata parsing and `SliceSummary` building
//! used by both [`super::prusa_gcode`] and [`super::prusa_bgcode`] —
//! PrusaSlicer emits the same key set in both places, just delivered
//! differently:
//!
//! - `prusa_gcode.rs` strips the leading `;` off each full-line comment in
//!   the plain-text footer itself.
//! - `prusa_bgcode.rs` gets it via `binarygcode`'s metadata block, which is
//!   already raw `key = value` text with no comment markers at all.
//!
//! (Earlier plan was to lean on the `gcode` crate's parsed `Comment`
//! values instead of scanning lines by hand — dropped after it choked on
//! a real sliced file: its full-program parser errors out on ordinary
//! firmware-specific commands like quoted `M117` strings, so a single
//! unrelated line anywhere in a multi-thousand-line file could take out
//! metadata extraction entirely. We only ever want the comment text, never
//! G-code semantics, so scanning lines directly is both simpler and more
//! robust.)
//!
//! This module takes a plain iterator of bare `key = value` lines either
//! way, so both callers feed it the same shape once they've stripped
//! their own container's framing. See `SPEC.md` §4.

use std::collections::HashMap;

use super::slice_summary::{FilamentUsage, PlateSummary, SliceSummary, SourceFormat};
use super::ParseError;

/// Parses `key = value` lines (already stripped of any comment marker)
/// into a map. Blank lines and lines without `=` are skipped.
pub fn parse_lines<'a>(lines: impl Iterator<Item = &'a str>) -> HashMap<String, String> {
    lines
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            let key = key.trim();
            if key.is_empty() {
                return None;
            }
            Some((key.to_string(), value.trim().to_string()))
        })
        .collect()
}

/// Builds a single-plate, single-slot `SliceSummary` from a parsed
/// metadata map — the shape both `.gcode` and `.bgcode` produce on a
/// single-extruder print. Multi-extruder (comma-separated `filament used
/// [g]` etc.) lands with milestone 6.
pub fn build_slice_summary(
    metadata: &HashMap<String, String>,
    source_format: SourceFormat,
) -> Result<SliceSummary, ParseError> {
    const TIME_KEY: &str = "estimated printing time (normal mode)";
    const GRAMS_KEY: &str = "filament used [g]";

    let print_time_seconds = metadata
        .get(TIME_KEY)
        .ok_or(ParseError::MissingField(TIME_KEY))
        .and_then(|s| parse_duration(s).ok_or_else(|| ParseError::InvalidField(TIME_KEY, s.clone())))?;

    let grams_raw = metadata.get(GRAMS_KEY).ok_or(ParseError::MissingField(GRAMS_KEY))?;
    let grams: f64 = grams_raw.parse().map_err(|_| ParseError::InvalidField(GRAMS_KEY, grams_raw.clone()))?;

    Ok(SliceSummary {
        source_format,
        plates: vec![PlateSummary {
            plate_index: 1,
            print_time_seconds,
            filament_usage: vec![FilamentUsage {
                slot_id: "1".to_string(),
                material_type: metadata.get("filament_type").cloned(),
                color: None,
                grams: Some(grams),
                ml: None,
            }],
            printer_model: metadata.get("printer_model").cloned(),
            layer_height: metadata.get("layer_height").and_then(|s| s.parse::<f64>().ok()),
            extra: HashMap::new(),
        }],
    })
}

/// "20m 7s" / "1h 5m 30s" style duration, as emitted after the
/// `estimated printing time` keys.
fn parse_duration(s: &str) -> Option<f64> {
    let mut total = 0.0;
    let mut matched = false;
    for token in s.split_whitespace() {
        let unit_start = token.find(|c: char| c.is_ascii_alphabetic())?;
        let (num, unit) = token.split_at(unit_start);
        let num: f64 = num.parse().ok()?;
        total += match unit {
            "h" => num * 3600.0,
            "m" => num * 60.0,
            "s" => num,
            _ => return None,
        };
        matched = true;
    }
    matched.then_some(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_lines_builds_a_map_and_skips_junk() {
        let lines = ["filament used [g] = 3.10", "printer_model = Prusa MINI", "", "no equals here"];
        let map = parse_lines(lines.into_iter());
        assert_eq!(map.get("filament used [g]").map(String::as_str), Some("3.10"));
        assert_eq!(map.get("printer_model").map(String::as_str), Some("Prusa MINI"));
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn parse_duration_handles_minutes_seconds_and_hours() {
        assert_eq!(parse_duration("20m 7s"), Some(1207.0));
        assert_eq!(parse_duration("1h 5m 30s"), Some(3930.0));
        assert_eq!(parse_duration("45s"), Some(45.0));
        assert_eq!(parse_duration(""), None);
        assert_eq!(parse_duration("garbage"), None);
    }

    #[test]
    fn build_slice_summary_requires_time_and_grams() {
        let mut metadata = HashMap::new();
        metadata.insert("filament used [g]".to_string(), "3.10".to_string());
        // missing the time key
        assert!(build_slice_summary(&metadata, SourceFormat::PrusaGcode).is_err());
    }
}
