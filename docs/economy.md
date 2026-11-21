# Economy content

How the era content in `config/businesses.yaml` and `config/customers.yaml` is derived. The
spec ([`spec.md`](spec.md) §7, §8) says what a business type and a shopping list are; this says
where the shipped numbers come from, so they can be regenerated rather than hand-tuned one by
one. Balance is then found by study (R64, R65): the study varies the economy's weights in
`config/economy.yaml`, not the formula.

## Economy weights (R71)

Every economy weight, every lever the balance study varies, lives in one file,
`config/economy.yaml`. Adopting an economic model means swapping that file:
`cp study-out/models/<id>/economy.yaml config/` ([`study.md`](study.md) "Overlays").

**The line.** A number that sets the *level* of the economy's money flows goes in
`economy.yaml`: what land, building and utilities cost, the lease term and how land is scored,
the money players start with and can borrow, what the city pays on a sell-back, how many people
there are and how much they spend, how sensitive they are to price, how much they want in each
decade, and the residential market's rules. A *description of the world* stays in its own file:
what exists and when (business types, goods, unlock and obsolete years), where (the map, block
land prices, tiers), what happens (events), and the relative pattern of who wants what and when
(the desire tables, segment multipliers, shopping windows, demographics, mortality, the
allowance's age and sex multipliers, class affinity). AI tunables stay in `ai.yaml`: they are AI
behaviour, not economy, and the main study holds them fixed. In practice: **a number the study
might tune goes in `economy.yaml`; if a new study parameter would vary a key elsewhere, move
the key here first** (a scenario fails if `study/study.yaml` varies anything outside it).

| Group | Key in `economy.yaml` | Was | Shipped |
|---|---|---|---|
| land | `land.base_rate` | `map.yaml` `land_base_rate` | £0.05 per unit of volume a day |
| land | `land.lease_term_years` | `game.yaml` `leases.term_years` | 30 |
| land | `land.value_method` | `game.yaml` `net_worth.land_value` | `book_decay` |
| land | `land.lot_property_uplift` | `game.yaml` `net_worth.lot_property_uplift` | 0.2 |
| building | `building.base_rate` | `businesses.yaml` `build_base_rate` | £10 per unit of volume |
| utilities | `utilities.base_rates.*` | `utilities.yaml` `base_rates` | 2p / 1p / 1p / 1p |
| utilities | `utilities.internet_base_rate` | `utilities.yaml` `internet.base_rate` | 2p |
| money | `money.starting_cash` | `game.yaml` `players.starting_cash` | £7,000 |
| money | `money.overdraft.*` | `game.yaml` `overdraft` | £10,000, +£2,500 a step, 0% |
| money | `money.sell_to_city_ratio` | `game.yaml` `bankruptcy.sell_to_city_ratio` | 0.5 |
| population | `population.births_multiplier` | new: × `customers.yaml` `births_per_woman` | 1.0 |
| population | `population.migration_per_day` | `customers.yaml` `migration_per_day` | 50 |
| population | `population.allowance_by_class` | `customers.yaml` `allowance.base_by_class` | £20 / £45 / £120 |
| population | `population.price_sensitivity` | `customers.yaml` `price_sensitivity` | 3.0 / 1.5 / 0.5 |
| demand | `demand_multipliers` | new: replaces the study's index addressing of `customers.desires` | 1.0 per decade |
| residential | `residential.*` | `customers.yaml` `residential`, all but `building_type` and `rehunt_unhoused` | as before |

Two judgement calls:

- **The utility rates move as they are**, four base rates and the internet rate, rather than
  collapsing into one `utility_rate_multiplier`. They are prices in whole pence and every one
  is a placeholder the study tunes (it scales them together today, but a model that prices
  water against electricity is a model too); none is a fact about the world. The year the
  exchange becomes the internet is a fact and stays in `utilities.yaml`, as do the
  nationalisation rules and the utilities' auction reserve prices.
- **Births and demand are multipliers**, because the tables they scale are content: the
  historical births per woman for England and Wales, and the era desire tables that say what a
  1930s shopper wants against a 2030s one. The study scales the level; the shape stays
  history. The residential market moves whole (the three numbers that were not study levers,
  `stretch_min_desirability`, `default_rent` and `reference_rent`, are market rules too), so
  the code reads one struct; `customers.yaml` keeps only which building type holds flats and
  whether the unhoused look again.

The move was a pure refactor: the shipped defaults moved verbatim, every multiplier is exactly
neutral at 1.0 (`P × 1.0` and `rate × 1.0000` are bit-identical), and the full shipped game
ends in the same state as before (`sim-cli run config --seed 42 --ticks 207360`: hash
`fd5bd3a7bcc696e4`, pinned in `crates/sim-cli/features/determinism.feature`; street routing has
since moved it on purpose, see "Streets and shop distance"). Scenarios:
`crates/sim-core/features/config/economy.feature`.

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
| Rent, utility and build rates | `economy.yaml` `land`, `utilities`, `building` | as shipped |

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

4. **The decade's demand multiplier** (`economy.yaml` `demand_multipliers`, R71): one per
   era, named by its first decade (`1930s`, `1950s`, … `2030s`), applied from that decade's
   first year until the next decade named, so `1930s` covers the 1930–45 tables. It multiplies
   `P(good)` after the segment rules, capped at 1, before unlock and fade
   (`customers::shopping::with_demand`). The shipped 1.0 is the tables as written. The study
   varies it by name, so reordering or adding desire rules never moves a study parameter onto
   the wrong table.

Allowances stay flat across eras (§8.2, R6: no inflation); what changes by decade is what
people want, not how much they have.

## Shopping windows (R70)

R70: each good is shopped for in a time window that depends on the shopper's age band, class
and sex. If the window passes without a purchase, the good is not bought that day and the
shopper goes on to their next item. The table is `customers.yaml` `shopping_windows`; the code is
`crates/sim-core/src/customers/windows.rs` (resolving the table) and `customers/errands.rs` (what a
shopper does next); the scenarios are `features/customers/windows.feature`.

**The table.** Rules apply in order, like `desires`: each rule whose match fits the shopper sets
the window of every good or group it names, so a later, narrower rule overrides an earlier,
broader one. A rule matches on `trade` (`day` or `night`, required), and optionally `class`,
`age`, `sex`, `from_year` and `until_year`. Within a rule `"*"` (every good) applies first and
replaces everything set before it, then the groups it names, then single goods. A window runs
from `from` up to (not including) `to`. Config validation rejects an unknown good, group or age
band, a window that does not start before it ends, a window outside its trade's hours, a group
named like a good, and any segment left without a window for a good it can want in any year
step of any preset (each error names the key).

**Groups** (shipped): papers (newspapers); food (bread, fruit & veg, meat, fish, dairy, pies,
cultured meat); treats (snacks, ice cream, coffee, vapes); fashion (clothes, second-hand
clothes, jewellery, grooming); shows (movies, theatre); late (dancing, drinks, craft beer, fast
food, fried chicken, games, escape games, VR).

**Shipped windows** (placeholders, in rule order; later rows override earlier ones for the goods
they name):

| Who | Trade | Years | Windows |
|---|---|---|---|
| Everyone | day | all | anything 09:00–17:00; the paper 07:00–10:00 |
| Everyone | night | all | anything 19:00–23:50; shows 19:00–21:00; meals 19:00–22:00 |
| Working class | day | all | anything 16:00–17:00 (after work); the paper 07:00–08:30; food and treats 12:00–14:00 (lunchtime) |
| Middle-class women | day | to 1965 | food 09:00–12:00 (the morning shop) |
| Middle class | day | from 1970 | food 12:00–14:00 (most adults at work) |
| Middle class 18–29 | day | all | fashion 15:00–17:00 |
| Upper class | day | all | anything 10:00–16:00; the paper 08:00–10:00 |
| 18–29 and 30–49 | day | from 1990 | coffee 07:00–10:00 |
| 65+ | day | all | anything 09:00–12:00; the paper 07:00–09:00 |
| Upper class 65+ | day | all | anything 10:00–11:00 |
| 18–29 | night | all | the late group 21:00–23:50 |
| 65+ | night | all | anything 19:00–21:30 |

**A shopper's day.** The rolled list is sorted by window (earliest opening, then earliest
closing). The shopper sets out at their earliest window's opening, trickling in over
`movement.spawn_spread_minutes` but never after half that window has gone. They try for a good
only inside its window. Between windows they walk home (residents) or back to their arrival
station (visitors), wait off the map there, and step back out from that spot when the next
window opens ("Streets and shop distance" below). A window that passes without a purchase
drops the good:

- **No shop can sell it now** (every candidate is dark, sold out, shut or too dear): dropped at
  once, since stock does not arrive mid-day (R2).
- **They reach the door after the window has closed:** dropped; no sale and no lost sale.
- **Sold out at the door:** a lost sale against that shop, as before (§8.3); they re-score while
  the window is open.

A dropped good is **not a lost sale against any shop**, so it does not touch reputation. Online
buying (§7.5, from 1995) ignores windows: shoppers roll for it once as they first set out. When
the list is done, residents go home, commuters take the train and tourists wander until the
hotels open (R42), as before.

**Power cuts (R69 with R70).** During a cut shoppers wait on the street, but only while the
window they are out for will still be open when the lights return. If the window closes in the
dark (or the lights come back exactly as it closes), they drop that good and plan their next
item; if their next window opens after the lights are back, they are away until it opens. As
shipped, the 1970 cuts (07:00–10:00 and 13:00–15:30) black out the morning paper's window for
everyone but upper-class pensioners, and leave working people's lunchtime food window
(12:00–14:00) only the hour before 13:00.

With every rule a whole-trade wildcard (`"*"` 07:00–17:00 by day, 19:00–23:50 by night),
shoppers behave exactly as before R70: the state hash of `sim-cli run --seed 42` matches the
pre-R70 build. The scenarios that are about something else (bankruptcy arithmetic, online
shares, the R69 waiting rule) use that table (`Given shoppers shop at any hour of trade`).

### What the windows cost

The retail probe from "Retail through the eras" (full 1930–2050 games, seeds 1–3, the default
roster; every walk-in shop's net profit as a share of its build cost a day), before and after
R70:

| Step | Before | After | | Decade | Before | After |
|---|---|---|---|---|---|---|
| 1960 | 11.8% | 12.4% | | 1930s | 5.7% | 5.7% |
| 1965 | 11.6% | 14.2% | | 1940s | 10.1% | 9.8% |
| **1970** (power cuts) | 11.1% | **10.3%** | | 1950s | 11.5% | 12.7% |
| 1975 | 9.5% | 10.5% | | 1960s | 11.7% | 13.3% |
| 1980 | 7.7% | 5.9% | | 1970s | 10.3% | 10.4% |
| | | | | 1980s | 6.5% | 5.8% |
| | | | | 1990s | 4.8% | 4.3% |
| | | | | 2000s | 6.7% | 6.1% |
| | | | | 2010s | 6.8% | 6.5% |
| | | | | 2020s | 0.2% | −0.2% |
| | | | | 2030s | 4.8% | 5.2% |
| | | | | 2040s | 5.6% | 6.5% |

The power cuts now cost something: the 1970 step was half a point below 1965 and is now four
points below it, because the morning paper's window is dark for nearly everyone and working
people's lunchtime food window has only an hour of light. A competent shop stays profitable in
every decade (the R68 lone-shop scenarios pass); the 2020s are the pandemic step as before, and
the all-retail average there moves from just above to just below zero. The middle-class
housewife's morning shop lifts the 1950s and 1960s.

`sim-cli study config --models 1 --games 600` (the shipped model, seats rotated), before → after:

| Seat | Win share | Median net worth | Bankrupt |
|---|---|---|---|
| The Grocer | 0% → 0% | £8,254 → £8,271 | 0% → 0% |
| The Landlord | 48.7% → 49.0% | £58,662 → £58,333 | 0% → 0% |
| The Toff | 0% → 0% | £7,752 → £7,753 | 0% → 0% |
| The Speculator | 0% → 0% | £11,771 → £11,898 | 3.3% → 1.7% |
| Sir Penny Sugar | 51.3% → 51.0% | £58,638 → £59,186 | 0% → 0% |
| Idle | 0% → 0% | £7,000 → £7,000 | 0% → 0% |

Balance is essentially unchanged: R70 moves retail by a point here and there, and retail was
not what decided these games (R65 is still unmet on the shipped config).

## Streets and shop distance (§4.1, R22, R25, R70)

Shoppers walk the streets: the one-cell road ring around every lot. They never cut through a
lot or a building, and they cross the Thames only on a bridge (`map.yaml` `boards.full.river`:
south of row 5, bridges at columns 1, 3, 4, 5 and 7; the bridge for column `c` carries the road
on that column's west side). A building's door is its frontage on the street, and a station's
entrance is the frontage of its block. How the routes are computed is in
[`architecture.md`](architecture.md) "Streets and walking".

**Distance for shop choice is the street route.** `distance_decay` (§8.3) reads the route's
length in blocks (cells over the block pitch), for shops and Hotels alike, instead of the
Manhattan distance. A shop across the river from a bridge is further away than it looks, and
so is one on the far side of a block.

**Between windows (R70)** a resident walks home and a visitor walks back to the station they
arrived at, and only then leaves the map; when their next window opens they step back out from
that spot. Visitors use their arrival station rather than a café: it is always there, it needs
no reading of the high street, and a commuter "at work" between errands is off the map by the
tube. A shopper's position never changes by more than `cells_per_tick` cells from one tick to
the next while they are on the map, so the walk mode never shows anyone appear or vanish
mid-street. The 17:00 close still clears day shoppers wherever they stand (§8.4).

The full shipped game moves on purpose: `sim-cli run config --seed 42 --ticks 207360` goes
from `fd5bd3a7bcc696e4` to `c2b3aca13bc5ee35` (pinned in
`crates/sim-cli/features/determinism.feature`).

`sim-cli study config --models 1 --games 600` (the shipped model, seats rotated), before → after:

| Seat | Win share | Median net worth | Bankrupt |
|---|---|---|---|
| The Grocer | 0% → 0% | £8,271 → £8,318 | 0% → 0% |
| The Landlord | 49.0% → 49.0% | £58,333 → £58,244 | 0% → 0% |
| The Toff | 0% → 0% | £7,753 → £7,758 | 0% → 0% |
| The Speculator | 0% → 0% | £11,898 → £11,767 | 1.7% → 2.0% |
| Sir Penny Sugar | 51.0% → 51.0% | £59,186 → £59,037 | 0% → 0% |
| Idle | 0% → 0% | £7,000 → £7,000 | 0% → 0% |

Balance is essentially unchanged: every median moves by under 1.2%, and the win shares not at
all. Longer walks shift which shop a shopper picks, not how much they spend, and the property
seats that decide these games barely feel it.

## Retail through the eras (R68)

R68: a competently run shop must be profitable in every era, and retail is fixed in the
economy, not by loosening the balance rule (R65). The formula above already makes a reference
shop pay at reference footfall on paper; what failed was the path from paper to the street.

### What was measured

Full 1930–2050 games on the shipped config (seeds 1–3, the default roster), every retail shop's
books read each night before the write-off, then per decade: takings, stock delivered (at
purchase price, binned waste included), rent, utilities, footfall and reputation. Alongside it,
one shop of reference size alone in the city, priced at reference and ordering what it sold
plus 10%.

- **Overheads were never the problem.** Rent and utilities came to £9–13 a day in every era,
  against £150–260 of takings for a greengrocer that was selling, as the formula intends.
- **Online shopping took nothing.** Spend only goes online when an online seller has stock
  (R15, §7.5); no seat built one in these games, so the online share was 0% in every decade.
- **Demand was there.** Shoppers wanted 680–990 units of fruit & veg a day in every decade,
  while the city's greengrocers had about 200–600 units on their shelves.
- **Shops died of their reputation.** A lost sale took a flat 0.02 off reputation and a sale
  added 0.005 × the gap to 1, so a shop that sold out lost more than it gained; and a
  reputation of 0 scores 0 (§8.3), so nobody chooses the shop, it makes no sale, and nothing
  can bring it back. The lone reference greengrocer sold out on its first trading day (66
  sold, 32 turned away), fell to 0 on its second and sold nothing for the rest of the game.
  In the full games a third of the Grocer's shops were dead by the 1940s, every one by the
  2000s; city-run shops inherited the dead reputations and binned a full order every day.
- **Orders by sales alone under-order.** A shop that sells out only sees what it had, so
  `sold × 1.1` grows 10% a day however many shoppers it turns away, and after any day with no
  sales it falls to one unit and climbs by one a day. With reputation fixed alone, the
  Grocer's shops sold 30–55 of their 238 units a day from the 1980s, sold out and turning
  shoppers away, while city-run shops ordering their full capacity sold 130–200.
- **A shut day was a dead loss.** The three-day week (§9) kept 4 shops in 7 shut for the whole
  1970 step, but they were billed for and binned that morning's delivery (R1), which no
  ordering can avoid: the order locked at the midnight the headline came out. R69 has since
  replaced whole-day closures with power cuts (below).

### The fix

1. **Reputation is a rolling stat in both directions** (§8.3). A lost sale now moves it
   `lost_sale_penalty` of the way to 0, the mirror of a sale moving it `sale_recovery` of the
   way to 1 (`customers::scoring::reputation_after_lost_sale`). It never reaches 0, so a shop
   that sold out is dented, not shut out for good; between rival shops it still settles where
   it reflects how often each turns shoppers away (`sales ÷ (sales + 4 × lost)` as shipped).
   The shipped 0.02 is kept and now reads as a share.
2. **No shop is shut all day** (§9, R69). At first a shop the three-day week kept shut took no
   delivery that day; R69 removed the whole-day closure instead. The 1970 step's power cuts
   darken every walk-in shop for a few hours (07:00–10:00 and 13:00–15:30 as shipped), the
   locked order arrives and is billed at 06:00 as on any day, and shoppers who find the lights
   out wait for them, so the stock still has the rest of the day to sell.
3. **AI orders count the shoppers turned away** (R2, R10). Tomorrow's order is
   `⌈(sold + lost sales) × (1 + order_margin)⌉`, lost sales counted only for goods that sold
   out (`ai::grocer::demand_seen`, used by every seat's evening orders). This is a fix to the
   AI's own competence, which the measurements showed wanting, not compensation for the economy.

### After the fix

The same probe, per decade, net profit as a share of build cost a day (all retail; the shops
are almost all the Grocer's 3×3×12 greengrocers on Low-tier land):

| Decade | Before | After | | Decade | Before | After |
|---|---|---|---|---|---|---|
| 1930s | 5.7% | 5.7% | | 1990s | −3.0% | 5.5% |
| 1940s | 6.6% | 10.1% | | 2000s | −2.8% | 6.2% |
| 1950s | 4.3% | 11.5% | | 2010s | −2.0% | 5.0% |
| 1960s | 1.6% | 11.7% | | 2020s | −2.7% | −0.4% |
| 1970s | −5.3% | 5.7% | | 2030s | −3.7% | 5.3% |
| 1980s | −4.4% | 7.7% | | 2040s | −5.6% | 5.9% |

The 2020s dip is the pandemic step (§9: physical retail demand −70% for a day), a shock no
order can see coming. A lone reference shop (`features/economy/retail_eras.feature`) makes a
profit in every decade 1930–2040: a greengrocer 19% of its build cost a day once its orders
have grown to capacity, a bakery 21%, a clothes store 17%.

### Power cuts instead of the three-day week (R69)

The same probe after R69 replaced the whole-day closure with power cuts (shops dark 07:00–10:00
and 13:00–15:30 on the 1970 step; the locked order arrives at 06:00):

| Step | Three-day week, shut shops take no delivery | Power cuts |
|---|---|---|
| 1965 | 11.6% | 11.6% |
| 1970 | 3.0% | 11.1% |
| 1975 | 8.3% | 9.5% |
| 1970s decade | 5.7% | 10.3% |

The cuts barely bite: shoppers who find the lights out wait for them, so the day's spending is
squeezed into the lit hours rather than lost, and a shop whose stock sells out in the lit hours
loses nothing. The lone reference shops sell their whole order on the 1970 step as on any
other. The 1970s are no longer the leanest decade for retail (the 2020s pandemic step is, at
about 0%); how hard the cuts should bite is a design lever (for instance, a share of shoppers
who give up and go home when the lights are out), not something R69 rules.

Retail still earns less than flats (21–28% of build cost a day in the measurements that
prompted R68), so this fixes R68, not R65: the residential seats still take every win on the
shipped config.

## Land value

Net worth (§3, R19, R39) counts a held lease's land. The spec gives one formula; how land
*should* be valued is not ruled, so `economy.yaml` `land.value_method` picks the method and the
balance study compares them (`study/study.yaml`, parameter `land_value`). The default keeps the
spec's formula, so the shipped game and its state hashes are unchanged. The code is
`crates/sim-core/src/economy/net_worth.rs` (`lease_land_value`, `market_land_price`); the
scenarios are `crates/sim-core/features/economy/land_value.feature`.

The **current land base price** of a block is its land base price today (the auction minimum,
R23, which a desirability shift moves) after every `land_value` event fired so far, applied in
order, each rounded to the penny (the 2008 crash: ×0.7). It is only read for net worth: what
an auction asks is untouched.

| Method | `value_method` | Character |
|---|---|---|
| `book_decay` (default) | `last auction price × years left / term` | The spec's formula. Land is worth what was paid, running down to nothing over the lease, like a depreciating asset. A crash scales the price paid. A lease loses half to two-thirds of its value over a 20-year game, so holding land is a slow loss on the scoreboard unless the business on it outruns the decay. |
| `market` | `current land base price × years left / term` | What the remaining years would cost today. Overpaying at auction is not rewarded in the score (the extra is gone from cash and does not reappear as land); a block that gentrifies gains, a crash loses. Still decays to nothing at the end of the lease. |
| `market_no_decay` | `current land base price` while the lease runs, 0 once it has run out | Land is a store of value: a lease counts at the block's full price until the day it ends, then drops to nothing (the block goes back to auction, R9). Holding land never loses score through time alone, only through shifts and crashes, and a cliff at expiry. |

Worked example (the scenarios): a 30-year lease on Bloomsbury (land base price £5,500) bought for
£6,000 in 1930; in 1935 the block gentrifies to £9,900.

| Scored for | Years left | `book_decay` | `market` | `market_no_decay` |
|---|---|---|---|---|
| 1930 | 30 | £6,000.00 | £5,500.00 | £5,500.00 |
| 1935 | 25 | £5,000.00 | £8,250.00 | £9,900.00 |
| 1955 | 5 | £1,000.00 | £1,650.00 | £9,900.00 |
| 1960 | 0 (back at auction) | £0.00 | £0.00 | £0.00 |

## Residential levers

Study levers, off by default. Over the full 1930–2050 span apartment rent is a guaranteed,
growing income: residents pay it out of allowances minted fresh every morning (R63), and the
city fills its flats as fast as they are built. Early studies found the residential AI (Sir
Penny Sugar) winning almost every game whatever else they varied, and none of them could rein
it in because no residential setting was varied. These rules give the study something to
search. They are **not rulings**: each is a choice for the designer, off in the shipped
config, so the shipped game and its state hashes are unchanged. Scenarios:
`crates/sim-core/features/population/housing.feature` (the end of the file) and
`crates/sim-core/features/economy/rent_levers.feature`.

The residential settings the study can vary, all in `economy.yaml` under `residential` (R71):

| Key | Shipped | Kind | Effect |
|---|---|---|---|
| `flat_size` | 4 | existing (R50) | units of volume per flat: larger flats, fewer of them per block |
| `comfortable_share` | 0.2 | existing (R52) | the rent share of allowance anyone will pay; the cheapest vacant flat must be within it |
| `stretch_share` | 0.5 | existing (R52) | how far a desirable flat can stretch that |
| `move_interval_years` | 10 | existing (R54) | years between moves |
| `reference_rent` | 3.00 | existing (R51) | the rent the price factor weighs flats against; the AIs' rents are multiples of it |
| `max_rent_per_flat` | `null` | **lever** | a cap on any flat's rent per day, in pounds |
| `rent_tax` | 0 | **lever** | share of residents' rent paid to the city |
| `commute_preference` | 0 | **lever** | share of the city's adults who commute rather than rent |

- **`max_rent_per_flat`** (`null` = no cap). An owner may still set any rent (R51), but what
  house-hunters see and residents pay is `min(rent, cap)`: the cap applies where flats are
  listed (`population::residential::capped_rent`), so the house-hunt, the move and the 06:00
  rent all use the capped figure. It bounds what a landlord who keeps raising rent on full
  blocks can collect.
- **`rent_tax`** (0–1). At 06:00 each block's rent is split: `rent_tax × rent` (rounded half
  away from zero) goes to the city, the rest to the owner (`RentDue::tax`,
  `economy::settlement::resident_rent_bills`). Residents pay the same; money is conserved.
  It scales every landlord's rent income by `1 − rent_tax`, the city's own blocks aside.
- **`commute_preference`** (0–1). Some people would rather commute in than live in a flat they
  can afford. Each morning's house-hunt houses no more than
  `⌊adults × (1 − commute_preference)⌋` in all, where adults are residents, the unhoused and
  that morning's arrivals (`population::residential::house_room`); nobody already housed is
  moved out. A per-arrival coin toss would not work: the unhoused re-hunt every morning while
  flats stand empty (`rehunt_unhoused`), so a declined arrival would be housed the next day
  and the lever would only delay filling. It only bites when flats would otherwise house more
  than that share: in a city short of flats it changes nothing.
