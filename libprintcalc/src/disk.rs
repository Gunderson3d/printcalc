//! Thin read-only disk helpers: load a sliced file, job, or profile from a
//! path. This is the only part of `libprintcalc` that touches the
//! filesystem — reading only; saving files, the profile library, and config
//! persistence stay in the CLI (`cli` feature). The in-memory counterparts
//! ([`crate::parse_slice_bytes`], [`crate::parse_job`]) remain the right
//! choice for callers without a filesystem (`printcalc-wasm`).

use std::fs;
use std::path::Path;

use crate::Error;
use crate::formats;
use crate::formats::slice_summary::SliceSummary;
use crate::model::filament::MaterialProfile;
use crate::model::job::{Job, JobSource};
use crate::model::printer::PrinterProfile;

impl Job {
    /// The job's slice data: cloned straight out of an `Embedded` source,
    /// or read and parsed from a `Path` source via [`read_slice_file`].
    ///
    /// A relative `source.path` resolves against the current working
    /// directory — if the job file lives elsewhere, resolve the path
    /// against its directory first (that's what the CLI does) and
    /// call [`read_slice_file`] yourself.
    ///
    /// On targets with no filesystem (`wasm32-unknown-unknown` and other
    /// non-WASI wasm), a `Path` source fails with
    /// [`Error::DiskUnavailable`] instead of attempting the read — embed
    /// the summary in the job there.
    pub fn slice_summary(&self) -> Result<SliceSummary, Error> {
        match &self.source {
            JobSource::Embedded(summary) => Ok(summary.clone()),
            JobSource::Path(path) => {
                if cfg!(all(target_family = "wasm", not(target_os = "wasi"))) {
                    return Err(Error::DiskUnavailable);
                }
                read_slice_file(path)
            }
        }
    }
}

/// Reads the sliced file at `path` and parses it via
/// [`formats::parse_slice_bytes`] — the file's own name is passed as the
/// detection hint.
pub fn read_slice_file(path: &Path) -> Result<SliceSummary, Error> {
    let bytes = fs::read(path)?;
    let filename = path.file_name().and_then(|f| f.to_str()).unwrap_or("");
    Ok(formats::parse_slice_bytes(&bytes, filename)?)
}

/// Reads and parses the job TOML file at `path` (see [`crate::parse_job`]).
pub fn read_job(path: &Path) -> Result<Job, Error> {
    crate::parse_job(&fs::read_to_string(path)?)
}

/// Reads and parses the printer profile TOML file at `path` (see
/// [`crate::parse_printer`]).
pub fn read_printer(path: &Path) -> Result<PrinterProfile, Error> {
    crate::parse_printer(&fs::read_to_string(path)?)
}

/// Reads and parses the filament/resin profile TOML file at `path` (see
/// [`crate::parse_material`]).
pub fn read_material(path: &Path) -> Result<MaterialProfile, Error> {
    crate::parse_material(&fs::read_to_string(path)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::filament::MaterialKind;
    use crate::model::printer::PrinterKind;

    /// Reads a real sliced file from disk, picking the parser from the
    /// filename extension.
    #[test]
    fn read_slice_file_parses_fixture() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../Testfiles/Shape-Box.sl1");
        let summary = read_slice_file(&path).unwrap();
        assert_eq!(summary.plates.len(), 1);
        assert!(summary.plates[0].print_time_seconds > 0.0);
    }

    /// Profiles round-trip through the same TOML the CLI's library saves,
    /// and a missing file surfaces as `Error::Io`.
    #[test]
    fn read_profiles_and_job_from_disk() {
        let dir = std::env::temp_dir().join("printcalc-test-disk");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let printer_path = dir.join("printer.toml");
        fs::write(
            &printer_path,
            r#"
name = "test-printer"
kind = "fdm"
purchase_price = 500.0
additional_upfront_cost = 0.0
estimated_life_years = 5.0
estimated_uptime_percent = 0.5
annual_maintenance_cost = 50.0
power_consumption_watts = 150.0
cost_buffer_factor = 1.2
"#,
        )
        .unwrap();
        let printer = read_printer(&printer_path).unwrap();
        assert_eq!(printer.name, "test-printer");
        assert_eq!(printer.kind, PrinterKind::Fdm);

        let material_path = dir.join("filament.toml");
        fs::write(
            &material_path,
            r#"
name = "test-pla"
kind = "filament"
price = 20.0
efficiency_factor = 1.1
"#,
        )
        .unwrap();
        let material = read_material(&material_path).unwrap();
        assert_eq!(material.kind, MaterialKind::Filament);

        assert!(matches!(
            read_job(&dir.join("nope.toml")).unwrap_err(),
            Error::Io(_)
        ));

        let _ = fs::remove_dir_all(&dir);
    }

    /// `Job::slice_summary` clones an embedded summary straight through and
    /// reads/parses a path source.
    #[test]
    fn job_slice_summary_handles_both_source_kinds() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../Testfiles/Shape-Box.sl1");
        let from_path = crate::new_job(
            "widget",
            path,
            crate::create_printer(
                "sl1-printer",
                PrinterKind::Resin,
                crate::WearProfile {
                    purchase_price: 400.0,
                    additional_upfront_cost: 0.0,
                    estimated_life_years: 5.0,
                    estimated_uptime_percent: 0.3,
                    annual_maintenance_cost: 20.0,
                    power_consumption_watts: 60.0,
                    cost_buffer_factor: 1.2,
                    print_time_rate_override: None,
                },
            ),
            std::collections::HashMap::new(),
            None,
            Vec::new(),
        );
        let summary = from_path.slice_summary().unwrap();
        assert_eq!(summary.plates.len(), 1);

        let embedded = Job {
            source: JobSource::Embedded(summary.clone()),
            ..from_path
        };
        assert_eq!(embedded.slice_summary().unwrap(), summary);
    }
}
