//! `SliceParser` trait, format detection, and per-slicer parsers.
//! See `SPEC.md` §2, §4.

use std::fmt;

pub mod slice_summary;

#[cfg(feature = "bambu-3mf")]
pub mod bambu_3mf;

#[cfg(feature = "prusa-gcode")]
pub mod prusa_gcode;

#[cfg(feature = "prusa-sl1")]
pub mod prusa_sl1;

#[cfg(feature = "prusa-bgcode")]
pub mod prusa_bgcode;

#[cfg(any(feature = "prusa-gcode", feature = "prusa-bgcode"))]
mod prusa_metadata;

use slice_summary::SliceSummary;

#[derive(Debug)]
pub enum ParseError {
    Io(std::io::Error),
    Zip(zip::result::ZipError),
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

impl From<zip::result::ZipError> for ParseError {
    fn from(e: zip::result::ZipError) -> Self {
        ParseError::Zip(e)
    }
}

/// Implemented by each per-slicer format module. See `SPEC.md` §2.
pub trait SliceParser {
    /// Cheap check for whether `bytes`/`filename` look like this parser's
    /// format, used to pick a parser before committing to a full [`Self::parse`].
    fn detect(bytes: &[u8], filename: &str) -> bool;
    fn parse(bytes: &[u8]) -> Result<SliceSummary, ParseError>;
}
