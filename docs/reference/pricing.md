# Pricing reference

How `printcalc` turns a sliced file into a landed cost and a suggested
sale price. This describes what the code does; it is the authority for
the arithmetic.

All money is in a single unspecified currency. The CLI prints `$`; no
conversion happens anywhere.

## Where the numbers come from

| Input | Source | Field |
|---|---|---|
| Print time | the sliced file | `PlateSummary::print_time_seconds` |
| Material used | the sliced file | `FilamentUsage::grams` / `::ml` |
| Machine cost inputs | printer profile | `WearProfile` |
| Material price and waste | filament profile | `MaterialProfile::price`, `::efficiency_factor` |
| Electricity, labor rate, margins | job override, else global config | `CostDefaults` |
| Labor time, extras, quantity | the job | `Job::labor`, `::hardware`, `::quantity` |

Print time and material used come from the slicer's own prediction. They
are never estimated.

## 1. Machine rate

What one hour of printing costs: depreciation plus electricity, with a
buffer factor over the raw depreciation math.

```
investment    = purchase_price + additional_upfront_cost
lifetime_cost = investment + annual_maintenance_cost × estimated_life_years
lifetime_hrs  = estimated_life_years × 8760 × estimated_uptime_percent

capital_per_hr  = lifetime_cost / lifetime_hrs
electric_per_hr = (power_consumption_watts / 1000) × electricity_cost_per_kwh

print_time_rate = (capital_per_hr + electric_per_hr) × cost_buffer_factor
```

`8760` is hours per year. A `lifetime_hrs` of zero — a zero-year life or
zero uptime — gives a `capital_per_hr` of zero, not a division error. A
profile can legitimately say "don't charge me for depreciation".

If the printer profile sets `print_time_rate_override`, it becomes
`print_time_rate` verbatim and every line above is skipped, buffer
factor included.

**Worked example.** A 699 machine, no extra upfront cost, 80/yr
maintenance, 5-year life at 50% uptime, 105W, 1.3 buffer, electricity at
0.14/kWh:

```
lifetime_cost   = 699 + 80 × 5           = 1099
lifetime_hrs    = 5 × 8760 × 0.50        = 21900
capital_per_hr  = 1099 / 21900           = 0.050183
electric_per_hr = 0.105 × 0.14           = 0.014700
print_time_rate = 0.064883 × 1.3         = 0.084347   per hour
```

## 2. Cost components

Every component is multiplied by `Job::quantity`, so each one — and the
total — is a **batch** figure. `unit_cost` is `total_cost / quantity`.

### Material

Each slot is priced against its own filament profile, so a multi-material
job can mix prices and waste factors:

```
for each used slot:
    amount = grams_or_ml × efficiency_factor × quantity
    cost   = amount × (price / 1000)

material_cost = Σ cost
```

`price / 1000` converts a per-kilogram price to per-gram, or a per-liter
price to per-milliliter. Which unit applies is fixed by the profile's
`kind`: `Filament` profiles price grams, `Resin` profiles price
milliliters. A mismatch between the profile kind and the sliced file's
format is rejected up front, so no unit conversion ever happens.

`efficiency_factor` is the waste allowance — purge, skirts, failed
prints. A job can override every profile's factor at once with
`material_efficiency_factor_override`.

A slot the slicer reports with zero grams *and* zero ml is loaded but
unused (an idle AMS tray, an unassigned extruder) and is skipped.

`material_breakdown` groups these lines by profile name, so slots sharing
a profile collapse into one line.

### Printer wear and electricity

One rate, reported as two lines:

```
print_hours       = total_print_time_seconds / 3600

wear_cost         = capital_per_hr  × cost_buffer_factor × print_hours × quantity
electricity_cost  = electric_per_hr × cost_buffer_factor × print_hours × quantity
```

Together these are `print_time_rate × print_hours × quantity`. They are
split only for display.

When `print_time_rate_override` is set, the whole rate is reported as
`wear_cost` and `electricity_cost` is zero. Splitting a stated flat rate
back into two halves would invent a division the user never gave.

### Labor

```
labor_cost = (labor.minutes / 60) × rate_per_hour × quantity
```

`labor.minutes` is hands-on time **per unit**. Charging a one-off setup
across a batch means dividing those minutes down yourself before entering
them.

`rate_per_hour` falls back to the global config default when the job
doesn't set one.

### Hardware

Flat extras — inserts, magnets, packaging:

```
for each item:  cost = item.cost × item.quantity × quantity
hardware_cost = Σ cost
```

`item.quantity` is the count **per unit**: four magnets per print stays
four however large the batch, and `Job::quantity` scales it.

Unlike material, hardware lines are never aggregated — each entry is
already its own line item.

## 3. Total and suggested price

```
total_cost = material_cost + wear_cost + electricity_cost
           + labor_cost + hardware_cost
```

This is cost. It contains no profit.

Sale prices are set by **gross margin** — the share of the sale price
that is not cost:

```
price(m) = total_cost / (1 - m)
```

Reported at each configured margin, `[50, 60, 70]` by default:

```
50%  →  total / 0.50  =  total × 2.000
60%  →  total / 0.40  =  total × 2.500
70%  →  total / 0.30  =  total × 3.333…
```

`total_cost` already includes `quantity`, so `price(m)` is the batch
price; `unit_price` is `price(m) / quantity`.

A margin of 100% or more is rejected with `CalcError::InvalidMargin`
rather than returning infinity. For why these are margins and not
markups, see [margins are not markups](../explanation/margins-not-markups.md).

## 4. Rounding

Full precision throughout. Rounding happens only when the result is
assembled.

| Value | Decimal places |
|---|---|
| `material_cost`, `wear_cost`, `electricity_cost`, `labor_cost`, `hardware_cost` | 4 |
| `total_cost`, `unit_cost`, `print_time_rate` | 4 |
| per-slot and per-hardware breakdown lines | 4 |
| `price`, `unit_price` | 2 |

`total_cost` is summed from the **unrounded** components, so the rounded
components may not add up to the rounded total in the last decimal.

## 5. Defaults

Any field a profile or job leaves out takes these values. See
[profile files](profile-files.md) and [job files](job-file.md) for where
each one is written.

**Printer** (`WearProfile`)

| Field | Unit | Default |
|---|---|---|
| `purchase_price` | currency | 800 |
| `additional_upfront_cost` | currency | 0 |
| `annual_maintenance_cost` | currency/yr | 80 |
| `estimated_life_years` | years | 5 |
| `estimated_uptime_percent` | 0.0–1.0 | 0.50 |
| `power_consumption_watts` | W | 160 |
| `cost_buffer_factor` | multiplier | 1.3 |
| `print_time_rate_override` | currency/hr | unset |

**Material** (`MaterialProfile`)

| Field | Unit | Default |
|---|---|---|
| `price` | currency/kg or /liter | 20 |
| `efficiency_factor` | multiplier | 1.5 |

**Job and operator**

| Field | Unit | Default |
|---|---|---|
| `quantity` | count | 1 |
| `labor.minutes` | minutes/unit | required |
| `labor.rate_per_hour` | currency/hr | 15 |
| `hardware[].quantity` | count/unit | 1 |
| `electricity_cost_per_kwh` | currency/kWh | 0.14 |
| `margin_percentages` | percent | `[50, 60, 70]` |

## 6. Errors

The calculation refuses rather than guessing. See `CalcError`:

| Error | Cause |
|---|---|
| `MissingMaterial` | A used slot has no profile and the job sets no `default_material`. Lists *every* unpriced slot at once, each named with its profile name, material type and color. |
| `PrinterKindMismatch` | An FDM printer profile against a resin file, or the reverse. |
| `FilamentKindMismatch` | A resin profile against an FDM file, or the reverse. |
| `PlateNotFound` | `plate_selection` names a plate the file doesn't contain. |
| `InvalidMargin` | A margin of 100% or more. |
| `ZeroQuantity` | `quantity` of 0. |

An unpriced slot is never silently costed at zero. See
[why matching refuses to guess](../explanation/conservative-matching.md).
