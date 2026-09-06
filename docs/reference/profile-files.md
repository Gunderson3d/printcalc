# Profile files

Printer and filament profiles are TOML files in your profile library.
They are reusable: a job refers to one by name rather than restating its
numbers.

## Location

```
~/.config/printcalc/
  printers/<name>.toml
  filaments/<name>.toml
  config.toml
```

The base directory is `$XDG_CONFIG_HOME/printcalc` on Linux and the
platform equivalent elsewhere. Set `PRINTCALC_CONFIG_DIR` to override it
— useful for a second profile set, a portable location, or scripting
against a throwaway library.

The filename stem must match the profile's `name` field; that stem is
what `--printer` and `--filament` look up.

## Printer profiles

```toml
name = "bambu-a1"
kind = "fdm"

purchase_price = 800.0
additional_upfront_cost = 0.0
estimated_life_years = 5.0
estimated_uptime_percent = 0.5
annual_maintenance_cost = 80.0
power_consumption_watts = 160.0
cost_buffer_factor = 1.3
```

| Key | Type | Default | Meaning |
|---|---|---|---|
| `name` | string | required | Also the filename stem. |
| `kind` | `"fdm"` \| `"resin"` | required | Must match the sliced file's process, or costing is rejected. |
| `purchase_price` | float | `800.0` | What the machine cost. |
| `additional_upfront_cost` | float | `0.0` | Enclosure, AMS, upgrades, tooling. |
| `estimated_life_years` | float | `5.0` | Depreciation horizon. |
| `estimated_uptime_percent` | float | `0.5` | Fraction of wall-clock time actually printing, `0.0`–`1.0`. |
| `annual_maintenance_cost` | float | `80.0` | Nozzles, belts, plates. |
| `power_consumption_watts` | float | `160.0` | Average draw while printing. |
| `cost_buffer_factor` | float | `1.3` | Safety margin over the raw depreciation math. |
| `print_time_rate_override` | float | unset | A flat cost per printing hour that replaces the entire computed rate — depreciation, electricity and buffer alike. |

Only `name` and `kind` are required. Everything else falls back to the
default above, so a profile can state just what you actually know:

```toml
name = "sparse"
kind = "fdm"
purchase_price = 699.0
power_consumption_watts = 105.0
```

Use `print_time_rate_override` when you already know what an hour on the
machine costs you and would rather state it than derive it:

```toml
name = "shop-rate"
kind = "fdm"
print_time_rate_override = 2.50
```

## Filament profiles

The same file shape covers filament and resin; `kind` decides how `price`
is read.

```toml
name = "pla-black"
kind = "filament"
price = 18.99
efficiency_factor = 1.5
color = "black"
vendor = "Prusa Polymers"
```

| Key | Type | Default | Meaning |
|---|---|---|---|
| `name` | string | required | Also the filename stem. |
| `kind` | `"filament"` \| `"resin"` | required | `filament` prices grams, `resin` prices milliliters. |
| `price` | float | `20.0` | Per kilogram for filament, per liter for resin. |
| `efficiency_factor` | float | `1.5` | Waste allowance — purge, skirts, failures. `1.5` prices 50% more than the slicer reported. |
| `color` | string | unset | Free-form; a name or a hex code. |
| `vendor` | string | unset | Manufacturer. Used by auto-detection to veto a wrong match. |

`color` and `vendor` do not affect cost. `vendor` does affect
[auto-detection](../explanation/conservative-matching.md): a stated
vendor that disagrees with the file's blocks a match outright.

## Global config

`config.toml` holds the fallbacks a job uses when it sets none of its
own.

```toml
electricity_cost_per_kwh = 0.14
labor_rate_per_hour = 15.0
margin_percentages = [50.0, 60.0, 70.0]
```

| Key | Type | Default |
|---|---|---|
| `electricity_cost_per_kwh` | float | `0.14` |
| `labor_rate_per_hour` | float | `15.0` |
| `margin_percentages` | float array | `[50.0, 60.0, 70.0]` |

`margin_percentages` are gross margins, not markups — see
[margins are not markups](../explanation/margins-not-markups.md). Files
written before the rename may spell the key `markup_percentages`; it
still loads, but the numbers now mean margins.

A missing or unreadable `config.toml` is not an error. A fresh install
uses the defaults above, so there is no init step.

Set these with `printcalc config`; see [the CLI reference](cli.md#printcalc-config).

## Built-in presets

The crate ships presets for common Bambu and Prusa hardware and
filaments. They are searched whenever a name doesn't resolve in your own
library, so `--printer prusa-mini-plus` works with an empty library.

Your own profiles always win: a saved profile shadows a built-in of the
same name, and auto-detection searches your library before the presets.

## Unrecognized keys

A key nothing recognizes is reported and ignored, not rejected:

```
warning: unrecognized keys in printers/a1.toml: purchase_pric (ignored)
```

The load still succeeds. The warning matters because the field the key
was meant to set silently took its default instead — a typo'd
`purchase_pric = 699.0` costs the job at 800.

Warnings go to stderr, so they never corrupt `--format json` output.
