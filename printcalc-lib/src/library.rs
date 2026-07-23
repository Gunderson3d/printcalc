//! Profile library load/save (`~/.config/printcalc/{printers,filaments}/*.toml`)
//! and global config (`~/.config/printcalc/config.toml`). See `SPEC.md` §6.
//!
//! Filesystem-only — not available to `printcalc-wasm`, which has no
//! filesystem and instead receives embedded profiles directly.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::model::filament::FilamentProfile;
use crate::model::printer::PrinterProfile;

#[derive(Debug)]
pub enum LibraryError {
    Io(std::io::Error),
    Toml(String),
    NotFound { kind: &'static str, name: String },
}

impl fmt::Display for LibraryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LibraryError::Io(e) => write!(f, "io error: {e}"),
            LibraryError::Toml(e) => write!(f, "toml error: {e}"),
            LibraryError::NotFound { kind, name } => write!(f, "{kind} {name:?} not found"),
        }
    }
}

impl std::error::Error for LibraryError {}

impl From<std::io::Error> for LibraryError {
    fn from(e: std::io::Error) -> Self {
        LibraryError::Io(e)
    }
}

/// `~/.config/printcalc/config.toml`. See `SPEC.md` §5.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlobalConfig {
    pub electricity_cost_per_kwh: f64,
    pub labor_rate_per_hour: f64,
    pub markup_percentages: Vec<f64>,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        GlobalConfig {
            electricity_cost_per_kwh: 0.14,
            labor_rate_per_hour: 15.0,
            markup_percentages: vec![50.0, 60.0, 70.0],
        }
    }
}

/// `~/.config/printcalc/` (via the `dirs` crate), or `$PRINTCALC_CONFIG_DIR`
/// if set — an override real users can reach for (a second profile set, a
/// portable/synced location) and tests use to avoid touching the real
/// user config.
pub fn config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("PRINTCALC_CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("printcalc")
}

pub fn printers_dir() -> PathBuf {
    config_dir().join("printers")
}

pub fn filaments_dir() -> PathBuf {
    config_dir().join("filaments")
}

/// Falls back to [`GlobalConfig::default`] if `config.toml` doesn't exist
/// yet — there's no `printcalc config init` step, a fresh install just
/// works with the defaults from `SPEC.md` §5.
pub fn load_global_config() -> Result<GlobalConfig, LibraryError> {
    let path = config_dir().join("config.toml");
    match fs::read_to_string(&path) {
        Ok(text) => toml::from_str(&text).map_err(|e| LibraryError::Toml(e.to_string())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(GlobalConfig::default()),
        Err(e) => Err(e.into()),
    }
}

fn save_profile<T: Serialize>(dir: &Path, name: &str, profile: &T) -> Result<(), LibraryError> {
    fs::create_dir_all(dir)?;
    let text = toml::to_string(profile).map_err(|e| LibraryError::Toml(e.to_string()))?;
    fs::write(dir.join(format!("{name}.toml")), text)?;
    Ok(())
}

fn load_profile<T: DeserializeOwned>(
    dir: &Path,
    kind: &'static str,
    name: &str,
) -> Result<T, LibraryError> {
    let path = dir.join(format!("{name}.toml"));
    let text = fs::read_to_string(&path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            LibraryError::NotFound { kind, name: name.to_string() }
        } else {
            LibraryError::Io(e)
        }
    })?;
    toml::from_str(&text).map_err(|e| LibraryError::Toml(e.to_string()))
}

fn list_profiles(dir: &Path) -> Result<Vec<String>, LibraryError> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut names: Vec<String> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|ext| ext.to_str()) == Some("toml"))
        .filter_map(|p| p.file_stem().and_then(|s| s.to_str()).map(String::from))
        .collect();
    names.sort();
    Ok(names)
}

pub fn save_printer(profile: &PrinterProfile) -> Result<(), LibraryError> {
    save_profile(&printers_dir(), &profile.name, profile)
}

pub fn load_printer(name: &str) -> Result<PrinterProfile, LibraryError> {
    load_profile(&printers_dir(), "printer", name)
}

pub fn list_printers() -> Result<Vec<String>, LibraryError> {
    list_profiles(&printers_dir())
}

pub fn save_filament(profile: &FilamentProfile) -> Result<(), LibraryError> {
    save_profile(&filaments_dir(), &profile.name, profile)
}

pub fn load_filament(name: &str) -> Result<FilamentProfile, LibraryError> {
    load_profile(&filaments_dir(), "filament", name)
}

pub fn list_filaments() -> Result<Vec<String>, LibraryError> {
    list_profiles(&filaments_dir())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::filament::MaterialKind;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("printcalc-lib-test-{name}"));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn save_load_and_list_round_trip() {
        let dir = temp_dir("save-load-list");
        let profile = FilamentProfile {
            name: "pla-black".into(),
            kind: MaterialKind::Filament,
            price_per_kg: Some(18.99),
            price_per_liter: None,
            density_g_per_cm3: None,
            efficiency_factor: 1.1,
            color: Some("black".into()),
            vendor: None,
        };

        save_profile(&dir, &profile.name, &profile).unwrap();
        let loaded: FilamentProfile = load_profile(&dir, "filament", "pla-black").unwrap();
        assert_eq!(loaded, profile);
        assert_eq!(list_profiles(&dir).unwrap(), vec!["pla-black".to_string()]);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_missing_profile_is_not_found() {
        let dir = temp_dir("missing");
        let err = load_profile::<FilamentProfile>(&dir, "filament", "nope").unwrap_err();
        assert!(matches!(
            err,
            LibraryError::NotFound { kind: "filament", name } if name == "nope"
        ));
    }

    #[test]
    fn list_of_missing_dir_is_empty() {
        let dir = temp_dir("list-missing-dir");
        assert_eq!(list_profiles(&dir).unwrap(), Vec::<String>::new());
    }
}
