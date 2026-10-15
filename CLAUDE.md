# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

`printcalc` costs a 3D print from the sliced file: material, print time,
printer depreciation, electricity, labor and per-unit hardware. Three
workspace crates: `libprintcalc` (all logic, no writes), `printcalc` (the
CLI, owns every write), `printcalc-wasm` (wasm-bindgen bindings, no logic).
`docs/explanation/architecture.md` is the authoritative design note; read it
before changing crate boundaries.

## Commands

CI (`.github/workflows/ci.yml`) runs exactly these, with warnings denied.
Run them all before pushing; the lint set in the root `Cargo.toml` is strict
(`unwrap_used`, `indexing_slicing`, `map_err_ignore`, `let_underscore_must_use`,
`float_cmp_const`, ...) and every crate opts in.

```sh
cargo fmt --all --check
RUSTFLAGS="-D warnings" cargo clippy --workspace --all-targets --all-features
cargo test --workspace                       # includes libprintcalc doctests
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
cargo build -p printcalc-wasm --target wasm32-unknown-unknown --release
cargo build -p libprintcalc --no-default-features --features gcode   # per-format matrix
```

Single test: `cargo test -p printcalc calc_job_resolves_bundled` (substring
match). `clippy.toml` relaxes unwrap/panic/indexing lints inside `#[cfg(test)]`
only.

Run the CLI against a fixture without touching `~/.config/printcalc`:

```sh
PRINTCALC_CONFIG_DIR=/tmp/pc-cfg ./target/debug/printcalc \
  "Testfiles/Cube_0.2mm_PLA_Prusa MINI_20m7s.gcode" --no-input
```

`--no-input` disables prompts; required values missing from flags become
errors naming the flag. `Testfiles/` has one fixture per supported format
plus a bundled job TOML; tests reference them by path from the crate root.

## Architecture rules that aren't obvious from one file

**Every number is decided in `libprintcalc`.** The CLI and the wasm crate
are frontends; if either needs new costing or parsing behaviour, it goes in
the library so the two can't disagree. The library never writes files,
creates directories or reads the environment. Writes, prompts, config and
the profile library (`~/.config/printcalc/{printers,filaments}/*.toml`,
`config.toml`) live in `printcalc/src/disk.rs`; `printcalc/src/ops.rs` is the
CLI's orchestration layer over `disk` and the library, and `cli/*.rs` is arg
parsing and terminal output only.

**The library's public API is `#[non_exhaustive]`.** Every public struct and
the `MaterialKind`/`PrinterKind`/`JobTier`/`SourceFormat` enums. Downstream
code (including `printcalc`, the wasm crate, and doctests, which compile as an
external crate) must build values with constructors and `with_*` setters
(`Job::new`, `MaterialProfile::new`, `WearProfile::new`, `HardwareItem::new`,
`SliceSummary::new`, `PlateSummary::new`, `FilamentUsage::grams`/`ml`, ...).
Struct literals and `..Default::default()` won't compile outside the crate.
Fields stay `pub` for reading and assignment. `ProfileRef` and `JobSource`
are deliberately exhaustive. Adding a public field means adding a setter.

**One `SliceSummary` for every format.** Parsers in
`libprintcalc/src/formats/` each sit behind a Cargo feature (`bambu-3mf`,
`gcode`, `prusa-bgcode`, `prusa-sl1`; `bambu`/`prusa` umbrellas) and
`parse_slice_bytes` dispatches through `#[cfg]`-guarded detect-then-parse
attempts. Downstream code knows nothing about the source format. Only
`print_time_seconds` and `grams`/`ml` feed costing; all other metadata exists
solely for auto-detection (`matching.rs`), which refuses to guess below a
confidence threshold rather than price a wrong profile.

**Jobs store inputs, never results.** A `Job` is file + profile refs +
quantity + extras; `calculate_cost` runs fresh each time. Hardware names
are unique per job: `Job::add_hardware` upserts by exact name, and the CLI
routes `--hardware` through it, so never `push`/`extend` `job.hardware`
directly. Profile refs are
`Linked { name }` (resolved against the saved library, then `builtin.rs`
presets) or `Embedded`. Job tiers only go up (`default` → `full` → `bundled`);
re-tiering downward is an error because names and paths are lost once
embedded. The slice path is stored absolute, and a relative one resolves
against the job file's directory.

**Margins, not markups.** `price = cost / (1 - margin)`. `--markup` is an
accepted alias whose numbers still mean margins. Values ≥ 100 are rejected in
`cli::parse_margins`, shared by `config --margin` and the per-job `--margin`.

**Defaults live in one place.** `WearProfile::default()` and the `DEFAULT_*`
constants beside it, plus `CostDefaults::default()`. Serde defaults and the
CLI prompts both read from there; never restate a default value elsewhere.

**Output discipline.** `--format json` prints `CostBreakdown` on stdout and
nothing else. Detection notes, warnings and the `wrote <path>` line go to
stderr. Receipt color is off when stdout isn't a terminal, under `--no-color`,
or when `NO_COLOR` is set; the currency symbol comes from config and is
display-only.

**Unknown TOML keys warn, never fail.** `parse_toml` returns the unknown
keys and `disk::warn_unknown_keys` prints them. `PrinterProfile` hand-rolls
`Deserialize` through a flat wire struct precisely so `#[serde(flatten)]`
can't swallow unknown keys; keep that pattern if you add flattened fields.

## Web demo

`printcalc-wasm/demo/` is a static page (Tailwind via CDN, `app.js` as an
ES module) over the wasm build; `pkg/` is generated and git-ignored.
`app.js` keeps its pure helpers (`buildJob`, `slotsFromSummary`,
`parseMargins`, formatters) exported and DOM-free so they run under Node
against the fixtures; `main()` only runs when `document` exists. The
wasm-bindgen CLI version must equal the `wasm-bindgen` crate version in
`Cargo.lock`. `.github/workflows/pages.yml` builds and publishes the demo
to GitHub Pages on pushes to `main` that touch the library or wasm crate.

## Docs

`docs/` follows Diátaxis (tutorial, how-to, reference, explanation). CLI
flags are documented in both the clap doc comments (which are the `--help`
text) and `docs/reference/cli.md`; change both. `PRICING.md` at the root is
the language-agnostic math; `docs/reference/pricing.md` is its in-tree
counterpart.

Commits and PR descriptions carry no attribution lines.
