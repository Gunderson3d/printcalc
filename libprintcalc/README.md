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
`parse_material` for in-memory data,
`read_slice_file`/`read_job`/`read_printer`/`read_material` for loading
straight from disk (read-only — saving stays in the CLI),
`create_printer`/`create_material` to build profiles in code,
`new_job` to build a job from a sliced-file path (or an embedded slice
summary) and a printer profile, and `calculate` to cost a job — all
re-exported at the root alongside the model types they operate on.

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

## License

[Apache-2.0](../LICENSE)
