# Pricing Math

The calculation, language-agnostic. Plain arithmetic on plain numbers.

## Symbols

**Job**

| Symbol | Unit | |
|---|---|---|
| `W` | g | total filament mass |
| `H` | h | total print time |
| `g[i]` | g | filament mass on slot `i` (multi-filament jobs) |
| `Q` | — | quantity (units in the batch) |
| `L` | min | hands-on labor, **per unit** |

`W` and `H` are the sums over all plates; `g[i]` is the sum of slot `i`'s grams
over all plates.

**Printer**

| Symbol | Unit | Default | |
|---|---|---|---|
| `C` | ¤ | 800 | purchase price |
| `A` | ¤ | 0 | additional upfront cost |
| `M` | ¤/yr | 80 | annual maintenance |
| `Y` | yr | 5 | life |
| `U` | 0–1 | 0.50 | uptime fraction |
| `P` | W | 160 | power draw while printing |
| `B` | × | 1.3 | cost buffer factor |
| `R̂` | ¤/h | — | rate override (optional) |

**Filament** (per preset)

| Symbol | Unit | Default | |
|---|---|---|---|
| `K` | ¤/kg | 20 | spool price |
| `m` | × | 1.5 | material markup (waste, purge, failures) |

**Operator**

| Symbol | Unit | Default | |
|---|---|---|---|
| `E` | ¤/kWh | 0.14 | electricity price |
| `r` | ¤/h | 15 | labor rate |
| `x[j]` | ¤ | — | extra cost `j`: unit cost |
| `n[j]` | — | 1 | extra cost `j`: count **per unit** |

---

## 1. Machine rate

```
investment   = C + A
lifetime     = investment + M·Y
hours        = Y · (8760 · U)          8760 = hours per year

capital/h    = lifetime / hours        (0 when hours = 0)
electric/h   = (P / 1000) · E

R            = (capital/h + electric/h) · B
```

`R̂`, if set, replaces `R` outright — the whole block above is skipped.

**Example** — `C=699, A=0, M=80, Y=5, U=0.50, P=105, B=1.3, E=0.14`

```
lifetime   = 699 + 400              = 1099
hours      = 5 · 4380               = 21900
capital/h  = 1099 / 21900           = 0.050183
electric/h = 0.105 · 0.14           = 0.014700
R          = 0.064883 · 1.3         = 0.084347   ¤/h
```

---

## 2. Components

Every component is multiplied by `Q`, so all four — and the total — are
**batch** figures.

**Filament.** One preset for the whole job:

```
F = (K / 1000) · W · m · Q
```

Or, when the job carries per-slot data, each slot priced with its own preset:

```
F = Σ over slots i:  (K[i] / 1000) · g[i] · m[i] · Q
```

`K / 1000` converts ¤/kg to ¤/g.

**Machine.**

```
Mc = R · H · Q
```

**Labor.**

```
Lc = (L / 60) · r · Q
```

`L` is per-unit, so `Q` scales it to the batch. Setup charged once per batch
means dividing those minutes by `Q` before entering them.

**Extras.**

```
X = Σ over entries j:  x[j] · n[j] · Q
```

`n[j]` is the count per unit; `Q` scales it to the batch. Entries are keyed
by name, so each name appears once per job.

---

## 3. Total and price

```
total = F + Mc + Lc + X
```

Price is set by **gross margin** — the fraction of the sale price that is not
cost:

```
price(m) = total / (1 - m)
```

Reported at three margins:

```
50%  →  total / 0.50  =  total · 2.000
60%  →  total / 0.40  =  total · 2.500
70%  →  total / 0.30  =  total · 3.333…
```

Divide, don't multiply: 60% *margin* means cost is 40% of price, so
`price = cost / 0.40`. Cost × 1.60 is a 60% *markup*, which is only a 37.5%
margin.

`total` already includes `Q`, so `price(m)` is the price for the whole batch;
per-unit is `price(m) / Q`.

---

## 4. Rounding

Full precision throughout; rounding only on output.

| | places |
|---|---|
| `F`, `Mc`, `Lc`, `X`, `total` | 4 |
| per-slot and per-extra lines | 4 |
| `price(m)` | 2 |

`total` is summed from the **unrounded** components, so the rounded parts may
not add up to the rounded total in the last decimal.
