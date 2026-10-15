# Job files

A job file is TOML describing one costable print job: which sliced file,
which profiles to price it against, and any job-specific overrides and
extras.

Write one with `-o`:

```sh
printcalc widget.gcode.3mf -p bambu-a1 -f pla-black -m 10 -o widget.job.toml
```

Cost it later by passing it back:

```sh
printcalc widget.job.toml
```

`printcalc` decides which it was given by file extension: `.toml` is a
job file, anything else is a sliced file.

## Minimal job

```toml
name = "widget"

[source]
path = "widget.gcode.3mf"

[printer]
ref = "bambu-a1"

[material]
ref = "pla-black"

[labor]
minutes = 10.0
```

## Top-level keys

| Key | Type | Default | Meaning |
|---|---|---|---|
| `name` | string | required | A label. Not used for anything but display. |
| `source` | table | required | The sliced file and which plate of it. See [Source](#source). |
| `printer` | table | required | Printer profile reference. See [Profile references](#profile-references). |
| `material` | table | — | Profile for a single-material job. Sugar for `materials.1`. |
| `materials` | table of tables | — | One profile per slot, keyed by slot id. |
| `default_material` | table | unset | Profile for any slot `materials` doesn't cover. |
| `quantity` | integer | `1` | Units in the batch. Scales every cost. |
| `labor` | table | required | See [Labor](#labor). |
| `hardware` | array of tables | `[]` | Flat extras. See [Hardware](#hardware). |
| `material_efficiency_factor_override` | float | unset | Replaces every profile's `efficiency_factor`. |
| `electricity_cost_per_kwh_override` | float | unset | Replaces the global config value. |
| `margin_percentages_override` | float array | unset | Replaces the global config margins. |

`material` and `materials` are mutually exclusive — give one or the
other, never both. A job with neither must set `default_material`.

## Source

Either a path to re-parse at cost time:

```toml
[source]
path = "widget.gcode.3mf"
```

A relative path resolves against **the job file's own directory**, not
the working directory, so a job and its sliced file can be moved together.

Or the parsed slice data embedded directly:

```toml
[source.summary]
source_format = "bambu-3mf"
# ... full SliceSummary
```

An embedded summary needs no sliced file at cost time. This is what
`--retier bundled` produces.

### Plate selection

Bambu `.gcode.3mf` files can hold several plates. `source.plate` chooses
between them:

```toml
[source]
path = "widget.gcode.3mf"
plate = 2
```

`plate` is either a plate number or the string `"all"`. Omitting it means
every plate, costed together. It is ignored with a warning for
single-plate formats.

## Profile references

Every profile reference — `printer`, each entry of `materials`,
`default_material` — takes one of two forms.

**Linked**, resolved by name at cost time against your library and then
the built-in presets:

```toml
[printer]
ref = "bambu-a1"
```

**Embedded**, carrying the profile's full data:

```toml
[printer.profile]
name = "bambu-a1"
kind = "fdm"
purchase_price = 800.0
# ... the rest of the profile
```

A linked job stays in sync with your library and breaks if the profile is
deleted. An embedded job is frozen and portable. See
[job tiers](../explanation/job-tiers.md) for choosing between them, and
`--retier` in [the CLI reference](cli.md) for converting.

## Materials

Single material — `[material]` is sugar for slot `"1"`:

```toml
[material]
ref = "pla-black"
```

Multi-material, keyed by the slot ids the slicer used (AMS tray number,
extruder index):

```toml
[materials.1]
ref = "bambu-pla-basic"

[materials.2]
ref = "bambu-pla-matte"
```

A fallback for anything not listed:

```toml
[default_material]
ref = "pla-black"
```

Without `default_material`, a used slot with no entry fails the
calculation and is named in the error. Nothing is priced at zero by
default.

## Labor

```toml
[labor]
minutes = 10.0
rate_per_hour = 22.0
```

| Key | Type | Default |
|---|---|---|
| `minutes` | float | required |
| `rate_per_hour` | float | global config (`15.0`) |

`minutes` is hands-on time **per unit**. It is multiplied by `quantity`,
so charging one setup across a batch means dividing the minutes down
yourself.

## Hardware

```toml
[[hardware]]
name = "M3 brass insert"
cost = 0.08
quantity = 4

[[hardware]]
name = "box"
cost = 0.60
```

| Key | Type | Default |
|---|---|---|
| `name` | string | required |
| `cost` | float | required |
| `quantity` | integer | `1` |

`quantity` is the count **per unit** — four inserts per print stays four
however large the batch, and the job's `quantity` scales it.

`printcalc ... -H "M3 brass insert:0.08:4"` writes the same entry from the
command line; see [`--hardware`](cli.md#job-inputs).

## Overrides

Each replaces a value that would otherwise come from a profile or the
global config, for this job only.

```toml
material_efficiency_factor_override = 1.2
electricity_cost_per_kwh_override = 0.31
margin_percentages_override = [40.0, 55.0]
```

`material_efficiency_factor_override` replaces the factor on *every*
material in the job, not one slot's.

`margin_percentages_override` are gross margins. The older spelling
`markup_percentages_override` still loads, but its numbers now mean
margins — see [margins are not markups](../explanation/margins-not-markups.md).

## Unrecognized keys

A key nothing recognizes is reported on stderr and ignored:

```
warning: unrecognized keys in widget.job.toml: bogus, hardware.0.qty (ignored)
```

The job still costs. The warning matters because the field the key was
meant to set silently took its default instead.

One blind spot: a stray key inside a `[printer]` or `[material]` block is
not reported, because those accept two different shapes and the parser
cannot tell a typo from the other shape. A wrong key there usually fails
the parse outright.
