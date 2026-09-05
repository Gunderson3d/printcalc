# libprintcalc

Library for calculating 3D print job costs — material, print time,
printer wear, electricity, labor, and extra hardware — from real
sliced-file output instead of manual entry. The `printcalc` crate is the
command-line tool built on it; `printcalc-wasm` in this workspace is a
thin browser frontend over it (see `../SPEC.md` for the full design).

## Usage

```sh
cargo add libprintcalc
```

Pick just the formats you need:

```sh
cargo add libprintcalc --no-default-features --features prusa-sl1
```

| Format | Feature |
|---|---|
| Bambu Studio `.gcode.3mf` | `bambu-3mf` |
| Plain-text `.gcode` (most FDM slicers) | `gcode` |
| PrusaSlicer binary `.bgcode` | `prusa-bgcode` |
| PrusaSlicer resin `.sl1`/`.sl1s` | `prusa-sl1` |

`bambu`/`prusa` are umbrella features pulling in every format for that
vendor; `default = ["bambu", "prusa"]` builds everything.

## Quick start

See the crate-level doc example (`cargo doc --open -p libprintcalc`, or
the top of `src/lib.rs`) for a runnable end-to-end example of the core
`calculate_cost` path. For a real application, the crate-root functions
are the entry points: `parse_slice_bytes`/`parse_job`/`parse_printer`/
`parse_material` for in-memory data (`parse_toml` does the same for any
profile or job type and additionally hands back every key the file
carried that no field claimed, so a caller can warn about a typo instead
of letting it silently take a default),
`read_slice_file`/`read_job`/`read_printer`/`read_material` for loading
straight from disk (read-only — saving stays in the CLI),
`create_printer`/`create_material` to build profiles in code,
`new_job` to build a job from a sliced-file path (or an embedded slice
summary) and a printer profile, `auto_job` to build one from the sliced
file alone (see below), and `calculate` to cost a job — all re-exported
at the root alongside the model types they operate on.

## Building a job from the file alone

Every supported format records which printer it was sliced for and what
sat in each filament slot — often down to the vendor's own profile name
and its price per kilo. `parse_slice_bytes` reads all of it into
`SliceSummary::meta` and each `FilamentUsage`, and `auto_job` turns that
into a costable `Job`:

```rust,ignore
let slice = libprintcalc::read_slice_file("widget.gcode.3mf".as_ref())?;
let auto = libprintcalc::auto_job("widget", slice.clone(), slice, [], [], 1.5);
let breakdown = libprintcalc::calculate(&auto.job, &Default::default())?;
```

The printer and each slot are matched against candidate profiles you
supply (your own library) and then the built-in presets; a slot that
matches nothing but whose file stated a price is costed from that. Every
guess comes back in `AutoJob` with its confidence and what it matched on,
so you can show it and let the user correct it — nothing here is
authoritative. A slot that ends up with no profile is left out of the
job, so costing it fails with `CalcError::MissingMaterial` rather than
quietly pricing it at zero. See `matching` for the scoring rules.

## Multi-plate / multi-filament

Bambu `.gcode.3mf` plates can be costed individually or combined, and any
format with more than one filament slot (Bambu AMS, Prusa XL/Core One
IDEX) is priced per slot against its own filament profile — see
`../SPEC.md` §2–§4 for the data model these map onto.

## No filesystem? No problem

`parse_slice_bytes` and `calculate_cost` do no I/O of
their own — pass bytes/data in, get a result out. Only the `read_*`
helpers in `disk` touch the filesystem, and they only read;
`printcalc-wasm` skips them entirely and calls the pure functions
directly.

## Pricing model

The arithmetic — machine rate, the five cost components, and margin-based
sale prices — is written out language-agnostically in
[`../PRICING.md`](../PRICING.md), including the mapping from its names to
this crate's.

## License

[Apache-2.0](../LICENSE)
