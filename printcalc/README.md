# printcalc

The `printcalc` command-line tool: calculates 3D print job costs —
material, print time, printer wear, electricity, labor, and extra
hardware — from real sliced-file output instead of manual entry. All
parsing and cost-calculation logic lives in the `libprintcalc` crate;
this crate is the CLI built on it. `printcalc-wasm` in this workspace is
a thin browser frontend over the library (see `../SPEC.md` for the full
design).

## Install

```sh
cargo install printcalc        # build from source
cargo binstall printcalc       # or grab a prebuilt binary
```

`printcalc <file> [flags]` costs a sliced file (or calcs an existing job
`.toml`); `printer`/`filament` subcommands manage your profile library.
Run `printcalc --help` for the full flag list.

Format support is feature-gated the same way as in `libprintcalc`
(`bambu-3mf`, `gcode`, `prusa-bgcode`, `prusa-sl1`, with `bambu`/`prusa`
umbrellas; `default = ["bambu", "prusa"]`) and passed through when
building from source:

```sh
cargo install printcalc --no-default-features --features prusa-sl1
```

## Library

Writing an application instead of using the CLI? Depend on
[`libprintcalc`](../libprintcalc) — see its README and crate docs
(`cargo doc --open -p libprintcalc`) for the API.

## License

[Apache-2.0](../LICENSE)
