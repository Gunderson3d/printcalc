# printcalc-wasm

`wasm-bindgen` bindings over [`libprintcalc`](../libprintcalc), exposing
everything that works without a filesystem. Price a dropped sliced file
entirely client-side.

## API

All arguments and returns are JSON strings, except `parse_slice_bytes`,
which takes raw bytes. `JSON.parse` the results on the JS side.

| Function | Returns |
|---|---|
| `parse_slice_bytes(bytes, filename)` | `SliceSummary` |
| `parse_job(job_toml)` | `Job` |
| `parse_printer(printer_toml)` | `PrinterProfile` |
| `parse_material(material_toml)` | `MaterialProfile` |
| `new_job(name, source, printer, materials, labor?, hardware?)` | `Job` |
| `auto_job(name, slice, printers?, materials?, efficiency_factor)` | `{ job, printer, slots }` |
| `calculate(job, defaults?)` | `CostBreakdown` |
| `calc_job(job_toml)` | `CostBreakdown`, one-shot from TOML |
| `builtin_printers()` / `builtin_filaments()` | preset arrays |
| `printcalc_wasm_version()` | version string |

The `disk` helpers from `libprintcalc` have no counterpart here.

## Usage

```js
import init, { parse_slice_bytes, auto_job, calculate } from "./pkg/printcalc_wasm.js";

await init();

const bytes = new Uint8Array(await file.arrayBuffer());
const slice = parse_slice_bytes(bytes, file.name);

const auto = JSON.parse(auto_job("widget", slice, null, null, 1.5));
// auto.printer and auto.slots each carry `confidence` and `matched_on` —
// show them and let the user correct before costing.

const breakdown = JSON.parse(calculate(JSON.stringify(auto.job), null));
console.log(breakdown.total_cost);
```

Pass the page's own saved profiles as JSON arrays in place of the two
`null`s and they are searched before the built-in presets.

`auto_job` never guesses a price: a slot that matches nothing and states
no price is left out, and costing fails naming it.

## Everything must be embedded

There is no filesystem here — no profile library to resolve a `ref =`
against, and no sliced file to re-read a `path =` from. Anything costed
in the browser needs embedded profiles (or built-in preset names) and an
embedded slice summary. `calc_job` requires a bundled-tier job for that
reason. See [job tiers](../docs/explanation/job-tiers.md).

Custom profiles go in as TOML through `parse_printer` / `parse_material`
and are embedded into the job JSON:

```js
job.printer = { profile: JSON.parse(parse_printer(printerToml)) };
job.material = { "1": { profile: JSON.parse(parse_material(filamentToml)) } };
```

## Building the demo

```sh
rustup target add wasm32-unknown-unknown          # once
cargo install wasm-bindgen-cli --version 0.2.126  # match Cargo.lock

cargo build -p printcalc-wasm --target wasm32-unknown-unknown --release
wasm-bindgen ../target/wasm32-unknown-unknown/release/printcalc_wasm.wasm \
  --out-dir demo/pkg --target web

# Serve over http:// — browsers block ES module imports and wasm fetch
# from file:// origins.
python3 -m http.server -d demo 8000
```

Then open `http://localhost:8000/`. `demo/pkg/` is a generated build
artifact (git-ignored); regenerate it after any change to this crate.

## Documentation

- [Run it in the browser](../docs/how-to/run-it-in-the-browser.md)
- [Architecture](../docs/explanation/architecture.md) — why the core does
  no I/O

## License

[Apache-2.0](../LICENSE)
