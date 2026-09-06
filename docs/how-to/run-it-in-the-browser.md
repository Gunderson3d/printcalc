# Run it in the browser

For pricing a dropped sliced file client-side, with no server.

`printcalc-wasm` is `wasm-bindgen` bindings over `libprintcalc`. It
exposes everything that works without a filesystem.

## Build it

```sh
rustup target add wasm32-unknown-unknown          # once
cargo install wasm-bindgen-cli --version 0.2.126  # match Cargo.lock

cargo build -p printcalc-wasm --target wasm32-unknown-unknown --release
wasm-bindgen target/wasm32-unknown-unknown/release/printcalc_wasm.wasm \
  --out-dir printcalc-wasm/demo/pkg --target web
```

The `wasm-bindgen-cli` version must match the `wasm-bindgen` version in
`Cargo.lock`, or the generated glue won't load.

## Serve it

Browsers block ES module imports and wasm fetches from `file://`, so it
has to go over HTTP:

```sh
python3 -m http.server -d printcalc-wasm/demo 8000
```

Then open `http://localhost:8000/`.

`demo/pkg/` is a generated build artifact and is git-ignored. Regenerate
it after any change to the crate.

## Price a dropped file

Every function returns a JSON string; `JSON.parse` it on the JS side.

```js
import init, { parse_slice_bytes, auto_job, calculate } from "./pkg/printcalc_wasm.js";

await init();

const bytes = new Uint8Array(await file.arrayBuffer());
const slice = parse_slice_bytes(bytes, file.name);

const auto = JSON.parse(auto_job("widget", slice, null, null, 1.5));

const breakdown = JSON.parse(calculate(JSON.stringify(auto.job), null));
console.log(breakdown.total_cost);
```

`auto_job` returns `{ job, printer, slots }`. Show `printer` and `slots`
— each carries a `confidence` and what it `matched_on` — and let the user
correct them before calling `calculate`.

## Use the page's own saved profiles

Pass them as JSON arrays and they are searched before the built-in
presets:

```js
const auto = JSON.parse(auto_job(
  "widget",
  slice,
  JSON.stringify(myPrinters),
  JSON.stringify(myFilaments),
  1.5,
));
```

`builtin_printers()` and `builtin_filaments()` return the shipped presets
as JSON arrays, if you want to show them in a picker.

## Accept custom profiles as TOML

Parse the TOML, then embed the result in the job:

```js
const printer = JSON.parse(parse_printer(printerToml));
job.printer = { profile: printer };
job.material = { "1": { profile: JSON.parse(parse_material(filamentToml)) } };
```

## What must be embedded

There is no filesystem in the browser: no profile library to resolve a
`ref =` against, and no sliced file to re-read a `path =` from. Anything
costed here needs **embedded profiles** (or built-in preset names) and an
**embedded slice summary**.

`calc_job(job_toml)` is a one-shot convenience for a complete job in TOML
form, and requires a bundled-tier job for exactly that reason. See
[job tiers](../explanation/job-tiers.md).

## Full API

| Function | Returns |
|---|---|
| `parse_slice_bytes(bytes, filename)` | `SliceSummary` |
| `parse_job(job_toml)` | `Job` |
| `parse_printer(printer_toml)` | `PrinterProfile` |
| `parse_material(material_toml)` | `MaterialProfile` |
| `new_job(name, source, printer, materials, labor?, hardware?)` | `Job` |
| `auto_job(name, slice, printers?, materials?, efficiency_factor)` | `{ job, printer, slots }` |
| `calculate(job, defaults?)` | `CostBreakdown` |
| `calc_job(job_toml)` | `CostBreakdown` |
| `builtin_printers()` / `builtin_filaments()` | preset arrays |
| `printcalc_wasm_version()` | version string |

All arguments and returns are JSON strings except `parse_slice_bytes`,
which takes raw bytes.

The `disk` helpers from `libprintcalc` have no counterpart here.

## See also

- [Embed the library](embed-the-library.md) — the same API in Rust.
- [Architecture](../explanation/architecture.md) — why the core does no
  I/O.
