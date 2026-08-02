//! `SliceParser` trait, format detection, and per-slicer parsers.

use std::fmt;

pub mod slice_summary;

#[cfg(feature = "bambu-3mf")]
pub mod bambu_3mf;

#[cfg(feature = "gcode")]
pub mod gcode;

#[cfg(feature = "prusa-sl1")]
pub mod prusa_sl1;

#[cfg(feature = "prusa-bgcode")]
pub mod prusa_bgcode;

#[cfg(any(feature = "gcode", feature = "prusa-bgcode"))]
mod gcode_metadata;

#[cfg(any(feature = "bambu-3mf", feature = "prusa-sl1"))]
mod zip_read;

use slice_summary::SliceSummary;

/// Why a [`SliceParser::parse`] call failed.
#[derive(Debug)]
#[non_exhaustive]
pub enum ParseError {
    /// Reading the file's bytes failed.
    Io(std::io::Error),
    /// The zip container (3MF, sl1/sl1s) couldn't be opened or an entry
    /// couldn't be inflated.
    #[cfg(any(feature = "bambu-3mf", feature = "prusa-sl1"))]
    Zip(String),
    /// The file doesn't look like this parser's format at all.
    NotThisFormat,
    /// A field this parser needs was missing from the sliced file.
    MissingField(&'static str),
    /// A field was present but couldn't be parsed as the expected type.
    InvalidField(&'static str, String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Io(e) => write!(f, "io error: {e}"),
            #[cfg(any(feature = "bambu-3mf", feature = "prusa-sl1"))]
            ParseError::Zip(e) => write!(f, "zip error: {e}"),
            ParseError::NotThisFormat => write!(f, "not this parser's format"),
            ParseError::MissingField(name) => write!(f, "missing field: {name}"),
            ParseError::InvalidField(name, value) => {
                write!(f, "invalid value for field {name}: {value:?}")
            }
        }
    }
}

impl std::error::Error for ParseError {}

impl From<std::io::Error> for ParseError {
    fn from(e: std::io::Error) -> Self {
        ParseError::Io(e)
    }
}

/// Implemented by each per-slicer format module.
pub trait SliceParser {
    /// Cheap check for whether `bytes`/`filename` look like this parser's
    /// format, used to pick a parser before committing to a full [`Self::parse`].
    fn detect(bytes: &[u8], filename: &str) -> bool;
    /// Parses `bytes` into a [`SliceSummary`]. Callers should have already
    /// checked [`Self::detect`], but implementations don't rely on it —
    /// a malformed match still fails with a proper [`ParseError`].
    fn parse(bytes: &[u8]) -> Result<SliceSummary, ParseError>;
}

/// Tries every parser compiled in (via feature flags) against `bytes`, and
/// parses with the first one whose `detect()` matches. Grows one `#[cfg]`
/// branch per parser as later milestones add them (bambu-3mf, gcode,
/// prusa-bgcode).
///
/// `filename` is a detection hint only (each parser checks its extension
/// first, then sniffs the bytes) — nothing is read from disk; for that see
/// [`crate::read_slice_file`].
pub fn parse_slice_bytes(bytes: &[u8], filename: &str) -> Result<SliceSummary, ParseError> {
    #[cfg(feature = "bambu-3mf")]
    if bambu_3mf::Bambu3mf::detect(bytes, filename) {
        return bambu_3mf::Bambu3mf::parse(bytes);
    }
    #[cfg(feature = "prusa-sl1")]
    if prusa_sl1::PrusaSl1::detect(bytes, filename) {
        return prusa_sl1::PrusaSl1::parse(bytes);
    }
    #[cfg(feature = "prusa-bgcode")]
    if prusa_bgcode::PrusaBgcode::detect(bytes, filename) {
        return prusa_bgcode::PrusaBgcode::parse(bytes);
    }
    #[cfg(feature = "gcode")]
    if gcode::Gcode::detect(bytes, filename) {
        return gcode::Gcode::parse(bytes);
    }
    let _ = (bytes, filename);
    Err(ParseError::NotThisFormat)
}
