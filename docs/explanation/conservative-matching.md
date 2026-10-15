# Why matching refuses to guess

Every supported sliced file records which printer it was sliced for and
which filament sat in each slot. `printcalc` reads those names and tries
to match them to profiles, which is why the common case needs no flags at
all.

Name matching is inherently fuzzy. The slicer writes
`Bambu PLA Basic @BBL P2S`; your profile is called `bambu-pla-basic`.
Something has to decide those are the same thing, and that something can
be wrong.

The design principle throughout is that **a wrong match is worse than no
match**. A refusal is visible: you get an error naming the slot, and you
fix it. A wrong match is invisible: you get a plausible number that is
quietly wrong, you quote it, and you find out months later. So the
matcher is built to fail loudly rather than succeed doubtfully.

## How names are compared

Names are reduced to sets of lowercase alphanumeric words, and the
`@printer` suffix is dropped — that part names the machine a preset is
tuned for, not the material, and keeping it would make two unrelated
filaments for the same printer look alike.

`Bambu PLA Basic @BBL P2S` becomes `{bambu, pla, basic}`; `bambu-pla-basic`
becomes the same set. Identical, so the match is exact.

The score blends two ratios: how much of the *shorter* name is shared,
weighted 0.85, and how much of the *longer* name is shared, weighted
0.15. Below 0.55, there is no match at all.

Weighting the shorter side heavily is deliberate. Your profile names are
usually shorter than the slicer's — `bambu-a1` inside
`Bambu Lab A1 0.4 nozzle` — and requiring full coverage of the slicer's
verbose name would reject nearly everything.

## Three deliberate refusals

### Only identical sets are "exact"

A name wholly contained in a longer one scores well but never reaches
1.0. `prusa-core-one` against `Prusa CORE One INDX 8T HF0.4 nozzle` is a
strong guess and is reported as `(guess)`, not `(exact)`.

That is the point of the 0.15 long-side weight. An earlier version scored
containment as a perfect match, and it confidently claimed a Core One
INDX was a plain Core One. The two are different machines with different
costs. Overclaiming certainty is a failure even when the match is
probably right, because it removes the reader's cue to check.

### A lone family name matches nothing

If one side reduces to a single word, it must be the whole of both sides
or it scores zero.

Without that rule, a profile named `PLA` matches every PLA filament in
existence — `pla-black`, `bambu-pla-basic`, `Generic PLA` — with a
perfect score, because the one word it has is fully covered. A single
family name identifies a *category*, not a product, and pricing against a
category is guessing.

### A disagreeing vendor is a hard veto

If both the file and the profile state a vendor and those vendors don't
match, the match is rejected outright regardless of how well the names
score.

`Generic PLA Silk` and `bambu-pla-silk` share two words of three and
would otherwise match comfortably. They are not the same filament and
don't cost the same. A stated vendor is strong evidence, and it is used
as evidence.

A vendor stated on only one side is not a veto — most profiles don't set
one, and absence isn't disagreement.

## When nothing matches

The fallback chain runs in order, and each step is narrower than the
last:

1. **Your saved profiles.** Searched first, so your own library always
   beats a shipped preset.
2. **The built-in presets.** Common Bambu and Prusa hardware and
   filaments, at US dollar list prices.
3. **The file's own stated price.** Most slicers record what the
   operator configured the spool to cost. That is a real number about
   this specific job, so it is used — reported as
   `priced from the file at $20.00/kg` so you can see where it came from.
   It is in whatever currency the slicer was configured in; no
   conversion is applied.
4. **Nothing.** The slot is left unpriced.

Step 4 is where the principle bites. An unpriced slot is not costed at
zero and not costed at a guessed average. It is left out of the job
entirely, so costing fails with an error naming the slot and everything
the file said about it:

```
error: no filament for slot 1 (Prusament Resin Model+ Prusa Orange @0.05
SL1S #FF8040); pass --filament as a default, or --filament-map
slot=name,... per slot
```

Every unpriced slot is reported at once rather than one per run, so a
multi-material job can be fixed in a single pass.

### The waste allowance is never invented

A profile built from the file's stated price still needs an
`efficiency_factor`, and no slicer records one. `printcalc` uses the
median of your saved filaments' factors, falling back to the documented
default when you have none saved. That is a stated assumption rather than
a discovered fact, which is why the tool tells you the price came from
the file.

### Resin usually can't be auto-priced

PrusaSlicer stores resin cost as a bottle price and a bottle volume, and
its stock presets ship with a bottle cost of `0`. Zero means *unpriced*,
not *free*, so the parser declines to treat it as a price.

Most SLA files therefore need a saved resin profile or an explicit
`--filament`. That is the correct outcome: the alternative is quoting
resin prints at the cost of the electricity alone.

## Everything is reviewable

Nothing the matcher decides is authoritative. The CLI prints each match
with its confidence label before the price. The library returns every
match in `AutoJob` with a numeric `confidence` and the string it
`matched_on`, so an application can show them and let the user correct
them before costing.

The matcher's job is to save you typing, not to make decisions for you.

## See also

- [Correct a wrong detection](../how-to/correct-a-wrong-detection.md).
- [Supported formats](../reference/supported-formats.md) — what each
  format records.
