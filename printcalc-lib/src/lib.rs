#![warn(missing_docs)]
//! Core library for calculating 3D print job costs from sliced-file output.
//!
//! `printcalc-lib` is the single home for all real logic — slice-file parsing,
//! cost calculation, profile CRUD, and job-tier conversion. `printcalc-cli` and
//! `printcalc-wasm` are thin frontends over [`ops`]; they should not need to
//! contain business logic of their own.
//!
//! Real applications will mostly want [`ops`] (parses a sliced file from
//! disk, resolves profiles against the local library, ...) or
//! [`formats::parse_slice_file`] plus [`model::cost::calculate_cost`]
//! directly if you're supplying everything in memory (as
//! `printcalc-wasm` does, with no filesystem to read from). The example
//! below shows the latter — the core computation with no I/O at all,
//! since that's the part that needs no real files on disk to demonstrate:
//!
//! ```
//! use std::collections::HashMap;
//!
//! use printcalc_lib::formats::slice_summary::{
//!     FilamentUsage, PlateSummary, SliceSummary, SourceFormat,
//! };
//! use printcalc_lib::disk::GlobalConfig;
//! use printcalc_lib::model::cost::calculate_cost;
//! use printcalc_lib::model::filament::{MaterialKind, MaterialProfile};
//! use printcalc_lib::model::job::{Job, JobSource, LaborCost, ProfileRef, SINGLE_MATERIAL_SLOT};
//! use printcalc_lib::model::printer::{PrinterKind, PrinterProfile, WearProfile};
//!
//! // Normally the output of `formats::parse_slice_file(bytes, filename)`.
//! let slice = SliceSummary {
//!     source_format: SourceFormat::Bambu3mf,
//!     plates: vec![PlateSummary {
//!         plate_index: 1,
//!         print_time_seconds: 1037.0,
//!         filament_usage: vec![FilamentUsage {
//!             slot_id: SINGLE_MATERIAL_SLOT.to_string(),
//!             material_type: Some("PLA".to_string()),
//!             color: None,
//!             grams: Some(6.66),
//!             ml: None,
//!         }],
//!         printer_model: None,
//!         layer_height: None,
//!         extra: HashMap::new(),
//!     }],
//! };
//!
//! let printer = PrinterProfile {
//!     name: "bambu-a1".to_string(),
//!     kind: PrinterKind::Fdm,
//!     wear: WearProfile {
//!         purchase_price: 800.0,
//!         additional_upfront_cost: 0.0,
//!         estimated_life_years: 5.0,
//!         estimated_uptime_percent: 0.5,
//!         annual_maintenance_cost: 80.0,
//!         power_consumption_watts: 160.0,
//!         cost_buffer_factor: 1.3,
//!     },
//! };
//! let filament = MaterialProfile {
//!     name: "pla-black".to_string(),
//!     kind: MaterialKind::Filament,
//!     price_per_kg: Some(18.99),
//!     price_per_liter: None,
//!     density_g_per_cm3: None,
//!     efficiency_factor: 1.1,
//!     color: None,
//!     vendor: None,
//! };
//!
//! let job = Job {
//!     name: "widget".to_string(),
//!     source: JobSource::Embedded(slice.clone()),
//!     plate_selection: None,
//!     printer: ProfileRef::Linked { name: "bambu-a1".to_string() },
//!     material: HashMap::from([(
//!         SINGLE_MATERIAL_SLOT.to_string(),
//!         ProfileRef::Linked { name: "pla-black".to_string() },
//!     )]),
//!     material_efficiency_factor_override: None,
//!     electricity_cost_per_kwh_override: None,
//!     labor: LaborCost { rate_per_hour: Some(15.0), minutes: 10.0 },
//!     hardware: vec![],
//!     markup_percentages_override: None,
//! };
//!
//! // In a real app, `ops::calc_job_data` resolves `job.printer`/`job.material`
//! // (which may be `Linked` refs) against the library first; here they're
//! // already resolved, so `calculate_cost` is called directly.
//! let material_profiles = HashMap::from([(SINGLE_MATERIAL_SLOT.to_string(), filament)]);
//! let breakdown =
//!     calculate_cost(&job, &slice, &printer, &material_profiles, &GlobalConfig::default())
//!         .expect("printer/filament kinds match the sliced file, and every used slot is priced");
//! assert!(breakdown.total_cost > 0.0);
//! ```

pub mod builtin;
pub mod disk;
pub mod formats;
pub mod model;
pub mod ops;

pub use ops::*;
