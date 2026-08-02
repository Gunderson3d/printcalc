//! Minimal zip reading shared by the container formats (3MF, sl1/sl1s):
//! locate one entry by name and inflate it, via `tinyzip` + `miniz_oxide`.
//! Slicer-produced archives are plain stored/deflate single-disk zips, which
//! is all `tinyzip` handles — anything else surfaces as a [`ParseError::Zip`].

use super::ParseError;

fn zip_err<E: core::fmt::Display>(e: tinyzip::Error<E>) -> ParseError {
    ParseError::Zip(e.to_string())
}

/// Whether `bytes` is a readable zip containing an entry named `path`.
pub fn has_entry(bytes: &[u8], path: &str) -> bool {
    if !bytes.starts_with(b"PK\x03\x04") {
        return false;
    }
    let Ok(archive) = tinyzip::Archive::open(bytes) else {
        return false;
    };
    archive.find_file(path).is_ok()
}

/// Reads and inflates the entry `path` as UTF-8 text.
pub fn read_entry_to_string(bytes: &[u8], path: &str) -> Result<String, ParseError> {
    let raw = read_entry(bytes, path)?;
    String::from_utf8(raw).map_err(|e| ParseError::Zip(format!("{path}: {e}")))
}

fn read_entry(bytes: &[u8], path: &str) -> Result<Vec<u8>, ParseError> {
    let archive = tinyzip::Archive::open(bytes).map_err(zip_err)?;
    let entry = archive.find_file(path).map_err(zip_err)?;
    let mut buf = vec![0u8; entry.compressed_size() as usize];
    let data = entry.read_to_slice(&mut buf).map_err(zip_err)?;
    match entry.compression().map_err(zip_err)? {
        tinyzip::Compression::Stored => Ok(data.to_vec()),
        tinyzip::Compression::Deflated => {
            // The central directory's size doubles as a zip-bomb cap.
            let limit = entry.uncompressed_size() as usize;
            miniz_oxide::inflate::decompress_to_vec_with_limit(data, limit)
                .map_err(|e| ParseError::Zip(format!("{path}: {e}")))
        }
    }
}
