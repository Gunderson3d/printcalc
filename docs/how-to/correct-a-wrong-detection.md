# Correct a wrong detection

For when `printcalc` picks the wrong printer or filament from what the
sliced file says.

## Read what it decided

The detected block reports every guess and how confident it is:

```
detected from part.gcode.3mf
  slicer:   BambuStudio 02.07.01.62
  printer:  Bambu Lab P2S 0.4 nozzle -> bambu-p2s (guess)
  slot 1 (Bambu PLA Basic @BBL P2S #AE6342) -> bambu-pla-basic (exact)
  slot 2 (Generic PLA @System #26A69A) -> priced from the file at $20.00/kg
```

| Label | Meaning |
|---|---|
| `(exact)` | The names agree completely. |
| `(guess)` | The names are close. Check it. |
| `priced from the file at $N/kg` | Nothing matched, so the price the slicer had configured was used. |
| `no matching profile` | Nothing matched and the file states no price. Costing will fail. |

Detection writes to stderr, so it never pollutes `--format json`.

## Override one thing

Naming a profile silently replaces the guess, and detection stops
reporting it:

```sh
printcalc part.gcode.3mf -p my-p2s
printcalc part.gcode.3mf -f my-pla
printcalc part.gcode.3mf --filament-map 2=my-pla
```

Precedence: `--filament-map`, then `--filament`, then detection.

## Override everything

`--no-detect` ignores what the file says entirely and behaves as though
it recorded nothing:

```sh
printcalc part.gcode.3mf --no-detect -p my-p2s -f my-pla
```

Without the flags it will prompt, or fail under `--no-input`. Use this
when the file's metadata is wrong rather than merely imprecise.

## Make the right profile win next time

Your own library is searched before the built-in presets, and a saved
profile shadows a built-in of the same name. Two ways to fix a
persistently wrong match:

**Name your profile what the file calls it.** Matching compares
lowercased word sets and ignores an `@printer` suffix, so a profile named
`bambu-pla-basic` matches `Bambu PLA Basic @BBL P2S` exactly.

```sh
printcalc filament add --name bambu-pla-basic --price 24.99
```

**Set the vendor.** A stated vendor that disagrees blocks a match
outright, which is the cleanest way to stop a generic filament matching a
branded preset:

```sh
printcalc filament add --name pla-silk --price 29.0 --vendor "Polymaker"
printcalc filament edit pla-silk --vendor "Polymaker"    # if it already exists
```

## Why a good-looking match still says "guess"

`(exact)` requires the two names to reduce to identical word sets.
`prusa-core-one` against `Prusa CORE One INDX 8T` is a strong match but
not an identical one, so it is reported as a guess. That is deliberate —
overclaiming certainty is worse than asking you to glance at it. See
[why matching refuses to guess](../explanation/conservative-matching.md).

## See also

- [CLI reference](../reference/cli.md#auto-detection).
- [Supported formats](../reference/supported-formats.md#what-gets-read) —
  what each format actually records.
