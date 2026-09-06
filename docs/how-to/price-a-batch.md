# Price a batch

For quoting more than one of the same thing.

## Multiply the whole job

```sh
printcalc widget.gcode -p mini -f pla-grey -m 12 -q 5
```

`-q/--quantity` scales all five cost components — material, wear,
electricity, labor and hardware — so every figure printed is for the
whole batch:

```
summary  (5 units)
  Material.................. $0.50
  Printer wear.............. $0.06
  Electricity............... $0.02
  Labor..................... $15.00
  Hardware.................. $0.00
  ─────────────────────────────
  TOTAL..................... $15.59
    per unit................ $3.12

  sale price @ 50% / 60% / 70% margin:  $31.17  $38.97  $51.96
  per unit:                             $6.23  $7.79  $10.39
```

The per-unit row is the batch figure divided by the quantity. Both are
shown so you can quote either without doing arithmetic.

## Charge setup once, not per unit

`--labor-minutes` is hands-on time **per unit**, and gets multiplied by
the quantity. That is right for per-print handling — removal, support
picking, finishing — and wrong for one-off setup.

To charge a fixed setup across a batch, divide it down yourself:

```sh
# 20 min setup once, plus 4 min handling per print, across 10 units
#   20/10 + 4 = 6
printcalc widget.gcode -m 6 -q 10 ...
```

## Add per-unit hardware

Extras are counted per unit too, and scale the same way. There is no CLI
flag; add them to a saved job file:

```sh
printcalc widget.gcode -p mini -f pla-grey -m 6 -q 10 -o batch.toml
```

Then edit `batch.toml`:

```toml
[[hardware]]
name = "M3 brass insert"
cost = 0.08
quantity = 4

[[hardware]]
name = "box"
cost = 0.60
```

```sh
printcalc batch.toml
```

Four inserts and one box **per print**, times ten prints. The `quantity`
in a hardware entry is per unit; the job's `quantity` scales it.

## Change the quantity later

Edit `quantity` in the saved job and re-cost it:

```toml
quantity = 25
```

```sh
printcalc batch.toml
```

Nothing else has to change.

## See also

- [Pricing reference](../reference/pricing.md#2-cost-components) — how
  quantity enters each component.
- [Job files: hardware](../reference/job-file.md#hardware).
