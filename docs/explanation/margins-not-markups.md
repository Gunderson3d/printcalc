# Margins are not markups

`printcalc` suggests sale prices by gross margin:

```
price = cost / (1 - margin)
```

Not by markup:

```
price = cost × (1 + markup)     # not what this tool does
```

The distinction matters more than it looks, and confusing the two is one
of the most common ways a small manufacturing business quietly underprices
itself.

## The two are different numbers

**Markup** is measured against cost. A 60% markup on a $10 part is
`10 × 1.60 = $16`.

**Margin** is measured against the sale price. A 60% margin means cost is
40% of what you charge, so the price is `10 / 0.40 = $25`.

Same percentage, very different prices:

| Percentage | As markup | As margin |
|---|---|---|
| 30% | 1.30× cost | 1.43× cost |
| 50% | 1.50× cost | 2.00× cost |
| 60% | 1.60× cost | 2.50× cost |
| 70% | 1.70× cost | 3.33× cost |

Run it backwards and the trap is clearer: a 60% *markup* is only a
**37.5% margin**. Someone who believes they are keeping 60% of revenue is
keeping well under half of it.

## Why this tool chose margin

Margin is what the rest of a business is denominated in. Rent, wages and
overhead are paid out of revenue, not out of cost, so "what fraction of
each sale is left over" is the number that has to cover them. A shop
targeting a 55% margin can compare that directly against a target from an
accountant, a competitor's published figures, or last quarter's books.

Markup can't be compared to anything without converting it first, and
converting it is exactly the step people skip.

## Why it is a division

The two conversions are:

```
price = cost / (1 - margin)
margin = 1 - cost / price
```

Dividing is what makes the margin a share of the *result* rather than a
share of the input. That is also why margin has a hard ceiling: as the
margin approaches 100%, `1 - margin` approaches zero and the price runs
away to infinity. A 100% margin would mean a sale with no cost at all.

`printcalc` rejects a margin of 100 or more rather than returning
infinity. There is no sensible answer to return, and a price of `inf`
printed in a quote is worse than an error.

## See also

- [Pricing reference](../reference/pricing.md#3-total-and-suggested-price)
  — the formula in context.
- [Set margins and rates](../how-to/set-margins-and-rates.md) — changing
  the tiers.
