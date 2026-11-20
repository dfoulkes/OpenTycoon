# Economy content

How the era content in `config/businesses.yaml` and `config/customers.yaml` is derived. The
spec ([`spec.md`](spec.md) §7, §8) says what a business type and a shopping list are; this says
where the shipped numbers come from, so they can be regenerated rather than hand-tuned one by
one. Balance is then found by study (R64, R65): the study varies these numbers, not the formula.

## Business stats

Every business type's stats are our own, derived from one small formula so they stay
internally consistent. It lives in `crates/sim-core/src/businesses/formula.rs`, in integer
money (pence) and ratios (basis points).

### Inputs

| Input | Where | Shipped |
|---|---|---|
| Size class → `min_footprint`, `max_height` | table below | per type |
| Utility profile → `utility_factor` | table below | per type |
| Margin class → `purchase_price = ref_price × (1 − margin)` | table below | per good |
| `ref_price` | `businesses.yaml` `goods` | chosen so a day's list fits a day's allowance (§8.2) |
| Reference height | `businesses.yaml` `reference.height` | 10 |
| Sell-through | `businesses.yaml` `reference.sell_through` | 0.8 |
| Payback days | `businesses.yaml` `reference.payback_days` | 10 |
| Rent, utility and build rates | `map.yaml`, `utilities.yaml`, `businesses.yaml` | as shipped |

**Size classes** (R58: the height cap is per type and never per era, R57):

| Class | `min_footprint` | `max_height` | Used for |
|---|---|---|---|
| kiosk | 1×2 | 6 | News Stand, Click & Collect Locker |
| small | 2×2 | 12 | bakery, butcher, café, jeweller, coffee chain, barber |
| medium | 2×3 | 16 | greengrocer, bookshop, bar, pharmacy, arcade, escape room |
| large | 3×3 | 20 | clothes store, restaurant, nightclub, gym, VR arcade |
| hall | 3×4 | 20 | furniture shop, ballroom, cinema, theatre, bowling alley |
| big | 4×6 | 30 | department store, supermarket |
| plant | 4×4 | 30 | data centre |
| tower | 3×3 | 40 | boutique hotel, vertical farm |
| yard | 3×4 | 6 | EV charging hub |

Apartments keep their own `2×2`, 100 high (R58: apartments can be towers).

**Utility profiles** (multipliers on the base rate per unit of volume):

| Profile | Electricity | Gas | Water | Telecoms | Used for |
|---|---|---|---|---|---|
| retail | 1.0 | 0.5 | 0.5 | 0.5 | most shops |
| kitchen | 1.0 | 2.0 | 1.5 | 0.5 | bakery, café, restaurant, fast food |
| chilled | 1.5 | 0.5 | 1.5 | 0.5 | butcher, dairy, greengrocer, supermarket |
| bar | 1.2 | 0.5 | 1.5 | 0.5 | bar, taproom |
| venue | 2.0 | 1.0 | 0.5 | 0.5 | ballroom, cinema, theatre, arcades |
| clinic | 1.0 | 0.5 | 1.5 | 1.5 | doctor's surgery, pharmacy, barber |
| office | 1.0 | 0.5 | 0.5 | 2.0 | travel agency, betting shop, co-working |
| electronic | 1.5 | 0.5 | 0.5 | 1.0 | electrical and computer shops |
| leisure | 1.5 | 1.0 | 1.5 | 0.5 | health club, gym |
| server | 4.0 | 0.2 | 1.0 | 3.0 | data centre, streaming, online casino |
| charger | 5.0 | 0.0 | 0.2 | 0.5 | EV charging hub |
| farm | 3.0 | 0.5 | 3.0 | 0.5 | vertical farm |
| hotel | 1.2 | 1.2 | 1.5 | 0.5 | boutique hotel |
| residential | 0.8 | 0.8 | 1.0 | 0.5 | apartments |

**Margin classes** (the retailer's gross margin on the reference price):

| Class | Margin | Goods |
|---|---|---|
| staple | 40% | bread, newspapers |
| fresh | 45% | fruit & veg, meat, fish, dairy, lab-grown meat |
| tech | 40% | computers, telephones, mobile phones |
| energy | 35% | EV charge |
| travel | 35% | package holidays |
| durable | 50% | clothes, books, furniture, toys, music, vapes |
| luxury | 55% | jewellery, antiques (`luxury: true`, §8.3) |
| prepared | 65% | tea & cake, pie & mash, meals, fast food, takeaway |
| drinks | 70% | drinks, coffee, craft beer |
| service | 80% | consultations, dancing, cinema, theatre, games, fitness |
| secondhand | 85% | second-hand clothes (donated stock) |
| digital | 90% | streams, online bets |

Every margin is above `1 − sell_through` (20%), so every good contributes something at the
reference sell-through.

### The formula

The **reference shop** of a type is a building of its `min_footprint`, `reference.height`
high, on a Medium-tier block at Medium fit-out, in its unlock year:

```
V          = min_footprint.w × min_footprint.d × reference.height
overheads  = rent(V, Medium) + Σ utilities(V, unlock_year)                  # §5.3, as settled at 06:00
target     = overheads + build_cost(V, Medium, Medium fit-out) ÷ payback_days
contrib(g) = sell_through × ref_price(g) − purchase_price(g)                  # per unit stocked
stock_density = target × goods ÷ (V × Σ contrib(g))                          # rounded up, 3 s.f.
```

So a reference shop that stocks its full capacity and sells `sell_through` of it (its
**reference footfall**) at reference prices covers its rent and utilities, pays for the stock
it bins at midnight (R1), and earns its build cost back in `payback_days`. Multi-good stores
split their capacity evenly (§7.1), hence `goods ÷ Σ`.

Consequences, by construction:

- A cheap good needs a dense shop (bread: 9.25 units per unit of volume); an expensive one a
  sparse shop (jewellery: 0.0775). Reference footfall is how many buyers a shop needs a day:
  a 2×2×10 bakery about 296 loaves, a jeweller 2 pieces.
- Overheads are a small part of the target (about 10%); the build-cost payback is most of it.
  So a reference shop clears its overheads on every tier (High rent is 1.8× Medium) and after
  the 1995 internet price rise, with room to spare.
- `payback_days` is the one knob for how lucrative shops are relative to building. Lowering it
  raises every density in proportion.

Example, the bakery (small, kitchen; bread £0.60, bought at £0.36):

```
V = 2 × 2 × 10 = 40
overheads = 40 × £0.05 (rent) + 40 × (2p × 1.0 + 1p × 2.0 + 1p × 1.5 + 1p × 0.5) = £2.00 + £2.40
target    = £4.40 + 40 × £10 ÷ 10 = £44.40
contrib   = 0.8 × £0.60 − £0.36 = £0.12
density   = £44.40 ÷ (40 × £0.12) = 9.25
```

### Checks

- `features/businesses/eras.feature`: a reference shop of every walk-in type clears rent,
  utilities and stock at reference footfall on every tier, from its unlock year to 2050
  (`formula::reference_daily_profit`), and a sample does so through the real build, order and
  sale path; the starting city (customers.yaml `initial_population`) wants at least one
  reference shop's footfall of every good in the type's unlock year.
- `tests/businesses_props.rs`: every shipped `stock_density` is within 1% above the formula,
  so a change to rents, utility rates or prices that isn't carried through shows up.
- `config/validate.rs` `check_eras`: unlock before obsolete, no type selling a good before it
  unlocks, online sellers only from the internet year (R15), the reference shop in range.

Regenerate the densities after changing an input: compute `formula::derived_stock_density`
for each type and round up to three significant figures.

## Era content

### Business types (§7.4)

71 types, 54 goods. Unlock years follow §7.4; obsolete years are where demand fades (§7.1).

| Era | New types | New goods |
|---|---|---|
| 1930 | 22: bakery, bookshop, butcher, café, clothes store, dairy, pie & mash shop, doctor's surgery, electrical shop, fishmonger, furniture shop, greengrocer (`grocery`), hardware shop, jeweller, news stand (obsolete 2010), souvenir shop, toy shop; ballroom (obsolete 1970), bar, cinema, restaurant, theatre; plus apartments | 22 |
| 1940 | antique shop, ice cream parlour, nightclub, pool hall | antiques, ice cream, games |
| 1950 | sports shop, music store (obsolete 2010), delicatessen, fast food outlet | sports goods, music (obsolete 2010), fast food |
| 1960 | travel agency (obsolete 2020), general store, pharmacy, bowling alley | holidays, medicine |
| 1970 | department store, health club, amusement arcade, video store (obsolete 2008) | videos (obsolete 2008), fitness |
| 1980 | computer shop, phone shop (obsolete 1995), supermarket, electronics superstore | computers, telephones (obsolete 1995) |
| 1990 | games software house, betting shop, charity shop, chicken shop, coffee chain, mobile phone shop | video games, bets, second-hand clothes, fried chicken, coffee, mobile phones |
| 1995 | internet café (obsolete 2015), e-commerce warehouse* | internet access (obsolete 2015) |
| 2000 | online casino*, gym, music streaming* | online bets, music streams |
| 2005 | boutique hotel, craft brewery & taproom | craft beer |
| 2010 | vape shop, video streaming*, food delivery platform* | vapes, video streams, takeaway |
| 2015 | co-working space, escape room, barber & beauty bar | desk days, escape games, grooming |
| 2020 | dark kitchen*, click & collect locker†, data centre† | — |
| 2025 | EV charging hub, vertical farm† | EV charge |
| 2030 | drone delivery hub†, VR arcade, robo-restaurant, lab-grown butcher | VR experiences, robot-cooked meals, lab-grown meat |

\* `channel: online` (§7.5). Declared here with their goods and stats; shoppers never walk to
them, and their demand model (the online share of each budget) belongs to the e-commerce work.
† `channel: infrastructure`: no goods and no walk-in trade; their effects (online capacity,
cheap produce, delivery) are later mechanics. The boutique hotel trades in the hotel window
(R30) and has no goods; room sales belong to the hotel work.

### Demand by decade (§8.2, R55, R64)

`customers.yaml` `desires` is `P(good | age, class, sex, hour, year)`:

1. **Era base tables**, one per window per era (1930–45, 1950–65, 1970–85, 1990–2005,
   2010–25, 2030–50), for the middle-class shopper. Staples dominate early (bread 0.55 and
   newspapers 0.55 a day in the 1930s); leisure, electronics and services rise later (coffee
   0.30, video streaming 0.30 in the 2030s).
2. **Segment scale rules** (`mode: scale`) for class, age and sex, applied in every era:
   working-class shoppers want less jewellery and more pie & mash; the upper class five times
   the antiques; 18–29s twice the dancing; 65+ twice the consultations. Results cap at 1.
3. **Unlock and fade**: a good is wanted only from its `unlock_year`; after its
   `obsolete_year` demand falls linearly over `obsolescence.fade_years` (15) to
   `obsolescence.floor` (10%) and stays there. A shop whose *type* is past its obsolete year
   draws shoppers with the same fade on its pull, so a news stand loses out to a general store
   selling the same papers.

Allowances stay flat across eras (§8.2, R6: no inflation); what changes by decade is what
people want, not how much they have.
