//! Data model: printer/filament profiles, jobs, and cost breakdowns.
//!
//! - [`printer`] and [`filament`] hold the reusable profiles a job is
//!   priced against.
//! - [`job`] is a costable job: which sliced file, which profiles, how
//!   many, and what extras. It holds no costs of its own.
//! - [`cost`] turns a job plus resolved profiles into a
//!   [`cost::CostBreakdown`]. The arithmetic is written out in
//!   `docs/reference/pricing.md`.
//!
//! Costing is a function, run fresh each time — there is no stored
//! result to invalidate when a profile or a quantity changes.

pub mod cost;
pub mod filament;
pub mod job;
pub mod printer;
