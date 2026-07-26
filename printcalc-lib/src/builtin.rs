//! Built-in printer/filament presets, embedded at compile time from
//! `builtin/{printers,filaments,resin}.toml` (via `include_str!`) — plain
//! data baked into the binary, no filesystem involved at runtime, so
//! (unlike [`crate::disk`]) these are available to `printcalc-wasm` too.
//! [`crate::ops::show_printer`]/[`crate::ops::show_filament`] (and
//! anything that resolves a `Linked` ref through them) fall back to
//! these when a name isn't found in the local library.
//!
//! Starter data — swap in a real catalog by editing those TOML files;
//! each is an array of profiles (`[[printer]]`/`[[material]]`), in the
//! same shape as a profile saved under `~/.config/printcalc/`.

use std::sync::LazyLock;

use serde::Deserialize;

use crate::model::filament::MaterialProfile;
use crate::model::printer::PrinterProfile;

#[derive(Deserialize)]
struct PrintersFile {
    printer: Vec<PrinterProfile>,
}

#[derive(Deserialize)]
struct FilamentsFile {
    material: Vec<MaterialProfile>,
}

fn parse_printers(toml_str: &str) -> Vec<PrinterProfile> {
    toml::from_str::<PrintersFile>(toml_str)
        .expect("builtin printer TOML is well-formed")
        .printer
}

fn parse_filaments(toml_str: &str) -> Vec<MaterialProfile> {
    toml::from_str::<FilamentsFile>(toml_str)
        .expect("builtin filament TOML is well-formed")
        .material
}

static PRINTERS: LazyLock<Vec<PrinterProfile>> =
    LazyLock::new(|| parse_printers(include_str!("../builtin/printers.toml")));

static FILAMENTS: LazyLock<Vec<MaterialProfile>> = LazyLock::new(|| {
    let mut filaments = parse_filaments(include_str!("../builtin/filaments.toml"));
    filaments.extend(parse_filaments(include_str!("../builtin/resin.toml")));
    filaments
});

/// Every built-in printer preset.
pub fn printers() -> &'static [PrinterProfile] {
    &PRINTERS
}

/// A built-in printer preset by name.
pub fn printer(name: &str) -> Option<&'static PrinterProfile> {
    PRINTERS.iter().find(|p| p.name == name)
}

/// Every built-in filament preset (filament and resin kinds alike).
pub fn filaments() -> &'static [MaterialProfile] {
    &FILAMENTS
}

/// A built-in filament preset by name.
pub fn filament(name: &str) -> Option<&'static MaterialProfile> {
    FILAMENTS.iter().find(|f| f.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_names_resolve_and_unknown_names_dont() {
        assert_eq!(
            printer("bambu-a1").unwrap().kind,
            crate::model::printer::PrinterKind::Fdm
        );
        assert!(printer("nonexistent").is_none());
        assert_eq!(
            filament("pla-black").unwrap().kind,
            crate::model::filament::MaterialKind::Filament
        );
        assert_eq!(
            filament("generic-resin-grey").unwrap().kind,
            crate::model::filament::MaterialKind::Resin
        );
        assert!(filament("nonexistent").is_none());
    }
}
