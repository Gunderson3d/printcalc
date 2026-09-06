# Architecture

Three crates in one workspace:

```
libprintcalc     parsing, matching, cost calculation. No writing.
printcalc        the CLI. Owns the profile library and all writes.
printcalc-wasm   wasm-bindgen bindings for the browser.
```

`printcalc` and `printcalc-wasm` are both thin frontends. Everything that
decides a number lives in `libprintcalc`, so the CLI and the browser
cannot disagree about what a job costs.

## The library does no writing

`libprintcalc` reads. It never writes a file, never creates a directory,
and never consults the environment. Saving profiles, saving jobs and
persisting config all live in the CLI.

That is a sharper line than "the library shouldn't have side effects",
and it is drawn where it is for a specific reason: **the profile library
is a CLI concept**. `~/.config/printcalc/printers/*.toml` is a convention
of one particular frontend. A web page has its own storage, and an
application embedding the crate has its own idea of where profiles live.
Baking a directory layout into the library would force both of them to
pretend to be a terminal.

So the library takes profiles as arguments and hands back results. Where
they came from is the caller's business.

## The core is I/O-free

Underneath the read helpers, the parts that actually compute take data
and return data:

- `parse_slice_bytes(bytes, filename)` — bytes in, `SliceSummary` out.
- `calculate_cost(job, slice, printer, materials, default, defaults)` —
  fully-resolved inputs in, `CostBreakdown` out.

Neither touches a filesystem. The `read_*` helpers in `disk` are thin
conveniences layered on top for callers that do have files.

This is what makes the browser build possible rather than merely
plausible. `printcalc-wasm` doesn't stub out a filesystem or ship a
virtual one; it skips `disk` entirely and calls the pure functions. The
wasm crate is bindings and JSON conversion, with no logic of its own —
which is the point, because logic in the bindings is logic the CLI
doesn't share.

The same property is why the library's own tests need almost no fixtures
on disk, and why `calculate_cost` can be exercised directly from a
doctest.

## Formats are feature-gated

Each parser is behind its own Cargo feature, with `bambu` and `prusa`
umbrellas over them. A resin-only shop can build with just `prusa-sl1`
and carry none of the 3MF or binary-G-code machinery.

The dispatch in `parse_slice_bytes` is a chain of `#[cfg]`-guarded
detect-then-parse attempts. A format that isn't compiled in simply isn't
in the chain, which is why an unreadable file and an unsupported one
report the same error — the parser that would have distinguished them
isn't there.

## Parsing produces one shape

All four formats produce the same `SliceSummary`: plates, print times,
and per-slot material usage, plus whatever identifying metadata the file
carried. Everything downstream — matching, costing, display — works
against that one shape and knows nothing about 3MF or G-code comments.

Adding a format means writing a parser and adding a feature. Nothing else
changes.

The metadata half of `SliceSummary` is strictly optional. Costing uses
only print time and material used; the printer profile names, vendors and
prices exist solely to feed [matching](conservative-matching.md). A
format that reports less still costs correctly, it just asks the user
more questions.

## Jobs separate what from how-much

A `Job` says which file, which profiles, how many, and what extras. It
does not contain costs. Costing is a function from a job plus resolved
profiles to a `CostBreakdown`, run fresh every time.

This is why editing `quantity` in a saved job and re-running gives a
correct answer with no recalculation step, and why a `default`-tier job
picks up a corrected profile automatically. There is no stored result to
invalidate.

## Where the numbers are defined

The default value for every profile field lives in exactly one place —
`WearProfile::default()` and the `DEFAULT_*` constants beside it — and
both the serde defaults and the CLI's prompts read from there. A default
written twice is a default that eventually disagrees with itself.

## See also

- [Embed the library](../how-to/embed-the-library.md).
- [Run it in the browser](../how-to/run-it-in-the-browser.md).
- [Pricing reference](../reference/pricing.md).
