> Design spec for OpenTycoon v2. Rulings are cited by ID (R1, R2, …) in §12; tests are tagged with them.

# London Tycoon: Game Spec (expanded draft)

>
> **Every constant in this spec is configurable in YAML**. The numbers here are the defaults shipped in `config/*.yaml`, never hard-coded. See §10b.

## 1. Premise

Real-time economic strategy for 2–6 players (human or AI) on a 9×9 grid of London districts. The game runs from **1930 to 2050**. Players lease land at auction, put up buildings, run businesses and charge rent. A simulated population of shoppers moves around the map and spends money. **The winner is the player with the highest net worth at the 2050 whistle**, or the last player still solvent.

The original game this borrows from was about Manhattan-style blocks. This one is pinned to London: real districts, the Blitz, decimalisation, elections, the Big Bang and the dot-com boom. That flavour is what makes it a different game, so lean into it.

## 2. Time

### 2.1 Clock

| Unit        | Value                                          | Notes                                                                                  |
| ----------- | ---------------------------------------------- | -------------------------------------------------------------------------------------- |
| In-game day | 24 game-hours                                  | **1 day = 20 real minutes**, so 1 game-hour = 50 real seconds. Configurable. |
| Year step   | **+5 years every midnight**. Every day is a new 5-year era. |
| Full game   | 1930 → 2050 = 120 yrs = 24 steps = **24 days** | At 20 min/day that's **8 real hours**, so see 2.4 |
| Pause       | Single-player only                             | **Single-player and multiplayer are both in scope**. **[PROPOSED]** Multiplayer gets speed votes (1×, 2×, 4×) instead of pause. |

### 2.2 Daily cycle

Trading hours: **day trade 07:00–17:00, night trade 19:00–23:50**. Every shop is shut by midnight, so midnight works as the single day boundary.

| Time | Event |
|---|---|
| **00:00 Reckoning** | Unsold stock is **written off as waste**. Solvency check (2.3). Tomorrow's stock orders lock. The year steps +5 here, every night. |
| 00:00–06:00 | Overnight. Nothing trades. |
| **06:00 Settlement** | Pay rent, utilities and stock. The locked stock order arrives. New population joins the map (§8.1). Balances may go negative. |
| **07:00–17:00** | Day trade |
| 17:00–19:00 | Changeover. Nothing trades. Last chance to set night prices. |
| **19:00–23:50** | Night trade |
| **23:30–03:00** | **Hotels**: rooms on sale. The 20-minute overlap with night trade is deliberate: shoppers who are done for the night start looking for a bed. Hotels have no stock, so nothing gets written off at midnight. |
| During trade | Players adjust prices and **tomorrow's** stock order |

So "restock can be adjusted during the day" means *tomorrow's order*. Stock only ever arrives at 06:00, and **there's no emergency mid-day restock**. Night shops get their stock at 06:00 too and sit on it until 19:00.

### 2.3 Bankruptcy

- At 00:00 each player's **cash balance** is checked. A balance **≥ £0 passes**, so exactly zero is solvent.
- Two consecutive failed midnights means the player is **bankrupt** and out of the game.
- The counter only resets on a *passed midnight*. Going positive during the day and spending it again doesn't count (The rule).
- Warning UI: **deferred** until the engine conversation. Working idea: the balance flashes.
- **What happens to a bankrupt player's assets**:
  - land leases go **back to the city**
  - buildings become **city-owned**
  - stock **goes to the city**
  - **The city runs those businesses at default settings** (default prices, default orders). **The city can't go bust.** The leases it takes over can be auctioned like any other unowned land, as **land only** (the buildings stay city-run). **[PROPOSED]** The city also puts its businesses up for auction one at a time.
- **Selling up**. This applies any time, not just in distress:
  - **Sell back to the city:** one click, instant, for **50% of true value** (property value, §3). The 50% is configurable.
  - **Auction a business:** businesses are auctioned **on their own, separately from land**. Same rules as land auctions (§5.1): same one-at-a-time queue, same 120 s countdown, **no shortcuts for distressed sellers**. The buyer takes over the business and pays rent to whoever holds the land.
  - **Auction land:** a land auction sells **the land only**. Any businesses on it keep their owners and pay rent to the new holder.

### 2.4 Game length presets [PROPOSED]

| Preset | Span | Days | Real time @20 min/day |
|---|---|---|---|
| Quick | 1930–1960 | 6 | 2h |
| Standard | 1930–1990 | 12 | 4h |
| Full | 1930–2050 | 24 | 8h |
| Modern | 1990–2050 | 12 | 4h (starts with the era unlocks already applied) |

Saving and resuming is mandatory for the Full preset.

## 3. Money

- **Currency: £, not $.** **[PROPOSED]** Pre-1971 prices show in £sd (pounds, shillings, pence) and flip to decimal on **Decimal Day, 15 Feb 1971**. That's display only: the engine stores pence internally.
- **No inflation**. Prices, rents and budgets don't drift over time. The economy grows because **population grows every morning**, which puts more money into circulation (§8.1). Every `I(year)` term from the earlier draft has been removed.
- **Starting cash**: every player starts with **£7,000**, configurable (`game.yaml` → `players.starting_cash`).
- **Overdraft**:
  - Every player has a credit limit of **£10,000**, rising by a **flat £2,500 every 5-year step (every day)** (25% of the starting limit). That reaches £70,000 by 2050.
  - Nothing in this draft charges interest on an overdraft.
  - Players can spend and bid down to `−credit_limit`.
  - **The 06:00 bills *can* push a player past the limit**. There's **no interest** on overdrafts. The midnight check is the punishment.
- **Net worth, the win metric**: **land value + property value + cash in the bank + stock**.
  - **Net worth is recalculated every midnight**, before waste is written off. That gives a nightly leaderboard, and the last one is the final score, so stock on hand on the final night counts.
  - **Utilities have no separate asset value.** What they're worth is what they save the holder and what they generate, and that already shows up in cash (§6).
  - **Lot value**: `lot_value = land_value + 20% × Σ value of every property on the lot`, whoever owns those properties. It goes to the **leaseholder**.
  - **Property value** goes to the **property's owner**.
  - If you hold the land *and* own the buildings on it, you count the property value plus the 20% uplift. That's the reward for owning both.
  - **Base values** (ruled: buildings don't wear):
    - land_value = `last auction price × remaining lease years / 30`
    - property value (also the "true value" used by sell-back) = `build cost`. **Buildings don't wear**.
    - stock = units on hand × purchase price

  ```
  net_worth = cash + stock
            + Σ (owned properties) property_value
            + Σ (held lots) [ land_value + 0.2 × Σ property_value on that lot ]
  ```

## 4. Map

### 4.1 Grid

- 9×9 = **81 blocks**, each with a district name, a **desirability tier** (Low / Medium / High) and a type (buildable, utility or special).
- Height cap: **100 units**.
- **Every block is a 9×9 lot**, and a lot holds **several businesses**. Each business takes a w×d footprint inside the lot.
  - **Every business must touch the edge of the lot** (street frontage).
  - Example: Old Kent Road as eight 3×3 businesses around the rim, with an unbuildable 3×3 gap in the middle.
  - One business can take the whole 9×9. With the 100 height cap, that's at most 8,100 units of volume.
  - The land auction is for the whole lot. The leaseholder collects rent from every business on it.
- Road segments between blocks carry the pedestrian pathfinding.
- **Sets**: blocks are grouped into colour sets, **laid out like a traditional board**. Eight colour bands run in price order. The cheapest and dearest bands are 2-block sets, and the six in between are 3-block sets. **Holding the leases on every block of a set unlocks the Hotel.**
- **Fitting the board onto 81 blocks**:

  | Band | Tier | Sets | Blocks |
  |---|---|---|---|
  | Brown | Low | 1 × 2 (e.g. Old Kent Road, Whitechapel) | 2 |
  | Light blue | Low | 4 × 3 | 12 |
  | Pink | Low | 4 × 3 | 12 |
  | Orange | Medium | 4 × 3 | 12 |
  | Red | Medium | 4 × 3 | 12 |
  | Yellow | Medium | 4 × 3 | 12 |
  | Green | High | 4 × 3 | 12 |
  | Dark blue | High | 1 × 2 (e.g. Mayfair, Belgravia) | 2 |
  | Utilities | — | Electricity, Gas, Water, Telephone/Internet (not a set) | 4 |
  | Centre landmark | — | City Hall, non-buildable. The city "player" lives here. | 1 |
  | **Total** | | **26 sets** | **81** |

  - Only **one** set each in brown and dark blue. Owning Mayfair stays as rare as it should be, and the cheap end is a genuine bargain bin.
  - A set's blocks are adjacent. Value falls from the centre outwards.
  - Tube stations sit *on* blocks as footfall points. They aren't blocks themselves.
  - The Thames runs between rows as a gap with bridges, not as a row of blocks.

### 4.2 Districts [PROPOSED starter set]

These are real places, chosen to give a spread of tiers. The centre of the grid is the West End and the City, and tiers fall away towards the edges.

| Tier | Districts |
|---|---|
| High | Mayfair, Belgravia, Knightsbridge, Kensington, Chelsea, St James's, Marylebone, Covent Garden, Hampstead |
| Medium | Soho, Holborn, Bloomsbury, Islington, Clerkenwell, Fitzrovia, Westminster, Southwark, Camden, Notting Hill, Fulham, Greenwich, Richmond, Pimlico |
| Low | Whitechapel, Bethnal Green, Hackney, Shoreditch, Bermondsey, Peckham, Brixton, Stepney, Poplar, Deptford, Old Kent Road, Elephant & Castle, Lambeth, Battersea, Wapping … |
| Special | The City (finance), Docklands (industrial → Canary Wharf after 1987), the Thames (non-buildable river row/column with bridges) |

**Desirability changes over time**:
- Shoreditch, Hackney and Brixton gentrify from Low to Medium/High between 1990 and 2010.
- Docklands goes from industrial to High after 1987.
- Notting Hill climbs during the 1980s.
- Blitz damage (1940–41) knocks East End blocks down.

Each shift is announced on the year step with a newspaper headline.

### 4.3 Transport nodes

Tube stations on fixed blocks are the **spawn and despawn points**. Customers arrive, shop, and leave. More lines open over time:
- Victoria line 1968
- Jubilee 1979 / 1999
- DLR 1987
- Elizabeth line 2022

Each new line boosts footfall on the blocks it serves.

## 5. Land

### 5.1 Auctions

- Any player can trigger an auction on an **unowned buildable block** by placing an opening bid. **The minimum opening bid is the district's land base price**. That price is set per district by how affluent it is: Mayfair very expensive, Old Kent Road cheap. It tracks the district's desirability.
- **Only one auction may be scheduled or running at a time** across the whole game.
- The auction starts **120 real seconds** after the opening bid. All players are notified and can see the countdown.
- Bidding goes **round the table, left to right**, starting with the player after the opener. On their turn a player raises by **£50 / £100 / £500 / £1,000** or passes.
- **Passing**:
  - A pass only counts for **that round**, so a player who passes can come back in next round.
  - If a full round goes by with **no new bids**, the auction closes and the highest bidder wins.
- Each turn has a **10-second timer**. If it runs out, that counts as a pass.
- **Bid ceiling**: `current balance + credit limit` (§3). The winner pays at once.
- The game doesn't pause during an auction. Shops keep trading.

### 5.2 Leases

- The winner holds the land **for 30 in-game years** (6 days).
- The leaseholder **collects land rent** from every building on the block, their own included. Rent on their own buildings nets to zero.
- **Unleased land** pays rent to the city.
- **When a lease expires, the block simply goes back to auction**. The old holder gets no bonus. Buildings stay put and pay rent to the new holder.
- **Leases can't be sold directly between players. They can only be auctioned**. The buyer gets the years left on the lease.

### 5.3 Formulas

```
land_rent/day    = (h × w × d)_building × land_base_rate × desirability_mult(tier)
utility_bill/day = Σ_utility (h × w × d)_building × utility_base_rate(u) × business_utility_factor(u)
build_cost       = (h × w × d)_building × build_base_rate × desirability_mult(tier)
stock_capacity   = (h × w × d)_building × business.stock_density
```

- The volume in these formulas is the **building's volume, not the plot's**. Volume also sets **how much stock the building can hold**.
- `desirability_mult` defaults: Low 0.6, Medium 1.0, High 1.8 (configurable).
- Utilities ignore desirability (from the original).

## 6. Utilities

- Pre-built from 1930: **Electricity, Gas, Water and a Telephone Exchange**. The exchange **upgrades to the Internet in 1995**. The telecoms column in the original tables stays valid.
- Each utility earns income from every building's utility bill (5.3), paid at the 06:00 settlement.
- **Each utility is a special block auctioned like land**. Its holder collects that utility's revenue across the whole map. **Their own buildings' bills for that utility net to zero**, so a utility both **generates** and **saves** money. Revenue from unheld utilities goes to the city.
- **Nationalisation and privatisation depend on who wins elections in the news cycle**. The proposed mechanic is §9.1.
- From **1995** every building pays a small internet bill, and online businesses need it.

## 7. Businesses

### 7.1 Model

**One building = one business**. The whole volume of a building serves its business type, with no stacking of floors.

Every business type has:

| Field | Meaning |
|---|---|
| `min_footprint` | Minimum w × d footprint within the 9×9 lot (must touch the lot edge) |
| `max_height` | **Maximum height for this business type, in units**. Configurable per type in `businesses.yaml`, capped by the global 100. E.g. a News Stand can't be a tower, but apartments and hotels can. |
| `stock_density` | Units of stock per unit of building volume |
| `hours` | Day (07:00–17:00) or Night (19:00–23:50) |
| `goods[]` | What it sells. Multi-good stores split their capacity. |
| `utility_factor{elec,gas,water,telecoms}` | Multiplier on the utility base rates |
| `unlock_year` | First year it can be built |
| `obsolete_year` | **[PROPOSED]** Optional. Demand fades after this year (News Stand 2010, Video Store 2008, Music Store 2010) |

**Daily running cost:** `rent + utilities + stock`. **There are no staff costs**.

**Player levers per shop:**
- price per good, as a % of the reference price
- tomorrow's order quantity
- open/closed toggle
- **fit-out quality** Low/Med/High. This shifts the shop's appeal to each class and its build cost.

### 7.2 Residential

**Residential is a building type: always apartments, never houses.** One building, one type. **There are no offices or office jobs in OpenTycoon.** Sir Penny Sugar (§10) lives here.

**Capacity:** `flats = building_volume ÷ flat_size`. Default `flat_size` = 4 units (about 64 m² of floor). Configurable.

**Costs:** land rent and utilities use the same volume formulas as every building (§5.3). No stock, and no trading window.

**Rent and demand:** the **owner sets the rent**, and demand works **the same way as for businesses** (§8.3):
- **Every class will pay more for a nicer lot in a nicer building** (district tier and fit-out).
- That pull is balanced against price.

**Who moves in: affordability rules.**
- Everyone *wants* to live in the city rather than commute or visit.
- Each person's daily rent budget comes from their **daily allowance** (§8.2):
  - **Comfortable:** rent ≤ **20%** of daily allowance.
  - **Stretch:** up to **50%**, but only if the flat's desirability is high enough to justify it.
  - **Priced out:** if the **cheapest available flat costs more than 20%**, they don't move in. They **commute or stay a tourist**.
- Among affordable flats they pick by the business scoring: desirability (tier, fit-out) against price, with the stretch allowance for the desirable ones.

**Arrivals (05:30):** the day's population growth **arrives by train at 05:30**. **Their first action of the day is finding an apartment**, before the 06:00 settlement. Those who find one become residents. Everyone else becomes a commuter or a tourist for the day.

**Moving:** existing residents can move **at most once every 10 in-game years** (every 2 days). They move only to somewhere with a better balance of desirability to affordability, under the same 20%/50% rule.

**[PROPOSED]** Rent is charged at the 06:00 settlement from the first morning a flat is occupied. A resident whose rent rises past 50% of their allowance moves out at their next move window, or immediately if they've already passed it.

### 7.3 The Hotel

Unlocked by holding every block of a set (§4.1). The Hotel serves a niche market: **tourists**.

**Tourists are the part of the population that doesn't live in the city:**

```
tourist_pool = total_population − housed_population
```

`housed_population` is everyone living in apartments (§7.2).

This creates a real tension: **every apartment built shrinks the tourist pool.** Sir Penny Sugar's residential empire and the hotel owners are working against each other. That's good conflict.

**Hotels are open 23:30–03:00, and they're the only business open in that window**.
- **No stock.** Like an apartment block, a hotel's **capacity (rooms) is set by its volume**.
- It sells rooms in that window. Rooms don't need restocking, so nothing is ever written off.
- **[PROPOSED]** Room revenue is booked at check-in. A room sold at 23:45 counts towards that night's midnight solvency check, and one sold at 01:00 counts towards the next day.

**How Hotels earn**:
- **Finding a bed**: a bed is a tourist's **last need of the day**. Once nothing left on their list can be bought that evening, they look for somewhere to sleep.
  - **They prefer a hotel.** Hotels compete for them on price, fit-out and the set's desirability.
  - **Before hotels open at 23:30**, a tourist who has finished their list **wanders the streets**. They still count as footfall but have nothing left to buy.
  - If no hotel room is available, **they take the train home** and leave at a tube station.
  - Residents go back to their apartments.
- A hotel earns room revenue per tourist it houses.
- Tourists shop as normal during the day (arriving by tube). The Souvenir Shop gets a bonus from tourists.
- One Hotel per set.

### 7.4 Unlock tables

- The first table is headed "Starting 1940" but lists the 1930 set.
- The 1980 table is pasted twice.
- The tables use $ and ¢.
- These numbers come almost word for word from the original game's manual. Fine as a placeholder, but see the IP note in §11.

Filling in the gaps **[PROPOSED]**:

| Year | New businesses | Notes |
|---|---|---|
| 1990 | Games Software House, Betting Shop, Charity Shop, Chicken Shop, **Coffee Chain**, **Mobile Phone Shop** (replaces Phone Shop) | Charity Shop: Low tier, discounted clothes, cheap stock from donations |
| 1995 | **Internet Café**, **E-commerce*** | E-commerce from 1995 |
| 2000 | Online Casino*, Gym, Music Streaming* | *Online (see 7.5) |
| 2005 | **Boutique Hotel**, **Craft Brewery/Taproom** | Night-time, High tier |
| 2010 | Vape Shop, Online Video Streaming*, **Food Delivery Platform*** | Delivery platform takes a cut from restaurants that opt in |
| 2015 | **Co-working Space**, **Escape Room**, **Barber/Beauty Bar** | |
| 2020 | **Dark Kitchen**, **Click & Collect Locker**, **Data Centre** | The data centre is an infrastructure building that raises the owner's online-business capacity |
| 2025 | **EV Charging Hub**, **Vertical Farm** | Vertical Farm supplies cheap produce to the owner's grocers |
| 2030+ | **Drone Delivery Hub**, **VR Arcade**, **Robo-Restaurant**, **Lab-grown Butcher** | Speculative era |

Every new entry needs the full field set from 7.1 before it's buildable.

### 7.5 Online businesses

The rule: **from 1995, customers can spend part of their daily budget at e-commerce businesses**. They don't travel to do it, and they can do it at any point in the day *instead of* walking to a shop. Whatever they spend online comes off what they have left for the high street.

**[PROPOSED]** mechanics:
- The share of each budget that goes online is `online_share(year, good)`. It starts small in 1995 and grows. Rather than every customer buying a bit online, each customer rolls once per good: buy online or walk.
- Online sellers need the Internet utility. Land tier barely matters to them, so they go well on cheap East End land. They compete on price plus a brand stat that builds over time.
- This **eats physical retail demand** in the late game. It's high-street decline as a mechanic.

## 8. Customers

### 8.1 Population

- `population(year)` follows a rough Greater London curve: dips around WW2 and in the 1970s–80s, then climbs. **It grows every morning at 06:00**. With no inflation, that daily growth is what drives the economy.
- Spawn sources:
  - **residences (apartments):** residents leave home for the day
  - **05:30 train arrivals:** the day's new population, house-hunting first (§7.2)
  - **tube stations:** shoppers arrive by travel
  - **visitors** (everyone not housed) arrive by tube and split into:
    - **commuters**, a configurable **70%** by default: they always take the train home at night and never want a hotel
    - **tourists**, the other 30%: they want a hotel and fall back to the train. **Only tourists count towards the hotel pool** (§7.3).
- Each customer has a **class**: Working / Middle / Upper. Class split by era, from 1930 W60/M30/U10 to 2050 W40/M45/U15.
- **Age bands**: **18–29, 30–49, 50–64, 65+**. Children aren't shoppers. Configurable.

#### Births (ruled: births per woman follow history)

Population is a **cohort model**: counts per age band and sex. Only adults are walking agents on the map. At each midnight year step (+5 years):
- **Births** = women aged 18–49 × the **historical births per woman** for that era ÷ the childbearing span (32 years, 18–49) × the 5 years in each step. Children are tracked as numbers only, not agents.
- **Ageing:** each cohort moves up a band as the years pass. **Children who reach 18 join the next 05:30 arrivals** and look for a flat like any newcomer.
- **Deaths:** a mortality rate per band and era. **Migration** (configurable) tops up the rest of the arrivals.
- The **inputs are historical; the population is not.** Total population comes out of the cohort model, so a city that can't house people doesn't grow the same way real London did.

Default births per woman by step: **approximate figures for England & Wales, accepted as defaults**. They're configurable in `customers.yaml`. 2025+ are projections:

```yaml
# customers.yaml
births_per_woman:
  1930: 1.9   1935: 1.8   1940: 1.9   1945: 2.4   # 1947 baby-boom spike ≈ 2.7
  1950: 2.2   1955: 2.4   1960: 2.7   1965: 2.9   # 1964 peak ≈ 2.95
  1970: 2.4   1975: 1.8   1980: 1.9   1985: 1.8
  1990: 1.8   1995: 1.7   2000: 1.6   2005: 1.8
  2010: 1.9   2015: 1.8   2020: 1.6   2025: 1.45
  2030: 1.5   2035: 1.5   2040: 1.5   2045: 1.5   2050: 1.5
```

### 8.2 Shopping list

Every person has a **daily allowance**. **It's new money created each morning**: there are no household accounts or wages, so allowances enter the economy fresh every day. The **size of the allowance and what they want to buy both depend on age group, class and sex**. Defaults go in `customers.yaml`:

```yaml
allowance:            # daily, before rent
  base_by_class: { working: 20, middle: 45, upper: 120 }
  age_multiplier: { "18-29": 0.9, "30-49": 1.1, "50-64": 1.0, "65+": 0.7 }
  sex_multiplier: { female: 1.0, male: 1.0 }
desires:              # P(good) weights per segment; tuned via sim-cli
  - match: { class: upper, age: "30-49" }
    goods: { jewellery: 0.08, restaurant_meal: 0.3, theatre: 0.15 }
demographics:         # population mix by era
  "1930": { class: [0.60, 0.30, 0.10], age: [0.30, 0.35, 0.20, 0.15], sex: [0.5, 0.5] }
```

Each person rolls their shopping list from `P(good | age, class, sex, hour, year)`. Rent comes out of the allowance first (residents), and online spend comes out of what's left (§7.5). Allowances are flat over time because there's no inflation. All numbers above are placeholders for tuning.

### 8.3 Shop choice

For each good on the list, the customer scores every reachable open shop with stock:

```
score = class_affinity(class, block_tier, shop_fit_out)
      × price_factor
      × distance_decay(walk_distance)
      × reputation(shop)

price_factor = exp(-k_class × (price/ref_price - 1))
```

- `k_class` is price sensitivity, defaulting to Working 3.0, Middle 1.5, Upper 0.5. Upper-class shoppers barely notice prices. Luxury goods (Jewellery, Antiques) get a mild boost from a higher price.
- The shop is picked by weighted random over the scores, not by taking the top score.
- If the chosen shop runs out before the customer arrives, that's a **lost sale**. It dents the shop's reputation and the customer re-scores.
- `reputation` is a 0–1 rolling stat.

### 8.4 Day/night

Night businesses only score while open. Customer generation shifts to the night-out goods (drinks, meals, dancing, movies) from 19:00. Day-only shoppers still on the map at 17:00 despawn. After night trade, tourists look for a bed or leave by train (§7.3), and residents go home.

## 9. Events

Announced on the year step as a newspaper front page. Some are fixed history, some random. The in-game year only moves in 5-year jumps, so **each event fires on the step that covers its year**: the Blitz on 1940, Decimal Day on 1970 (covering 1970–74), the crash on 2005 (covering 2005–09). Events live in `events.yaml`.

| Year | Event | Effect |
|---|---|---|
| 1939–45 | WWII and rationing | Luxury goods demand −60%; meat, dairy and sugar capped |
| 1940–41 | **The Blitz** | Random East End and City buildings destroyed. War-damage compensation at 50% |
| 1953 | Coronation | One-day souvenir and bar boom |
| 1966 | World Cup | Pubs and bars ×2 for a day |
| 1971 | Decimal Day | Currency display flips; one day of price confusion (random ±10%) |
| 1973–74 | Oil crisis / three-day week | Electricity rationed: businesses can only open 3 of 7 days |
| 1986 | Big Bang | The City's desirability jumps; Upper-class share rises |
| 1987 | Docklands / Canary Wharf | Docklands desirability: Low → High |
| 2000 | Dot-com bubble → 2001 bust | Online business valuations swing |
| 2008 | Financial crash | Land values −30%, Upper-class spend −20% for the day |
| 2012 | Olympics | East London footfall boost |
| 2020 | Pandemic | Physical retail demand −70% for 1 day; online ×2 |
| 2030+ | Random: heatwave, AI boom, flood (Thames Barrier fails?) | Flavour |

Random events (fire, burglary, health inspection) roll per building per day with a small chance. Insurance is an optional product.

### 9.1 Elections (ruled: go with the historically correct version)

Each 5-year step happens to match the length of a UK parliament, so **every year step is a general election**. The result is a weighted roll and is the paper's headline. Utility ownership follows the winner:
- **Labour win:** utilities are **nationalised**. Holders are bought out at book value and the revenue goes to the city.
- **Conservative win:** nationalised utilities are **privatised** and re-auctioned.
- Default weights follow history: Labour very likely in 1945, Conservatives favoured through the 1980s, a coin toss after 2025.

Historical footnote: Labour nationalised gas and electricity (1948–49) and the Tories privatised them (1986, 1990). "Only if the Conservatives get in" fits privatisation, not nationalisation.

## 10. AI opponents [PROPOSED]

Personalities give variety without much complexity:
- **The Landlord** hoards leases and never builds retail.
- **The Grocer** opens many cheap Low-tier shops.
- **The Toff** only goes for High tier.
- **The Speculator** bids hard and flips.
- **Sir Penny Sugar** deals mostly in residential. Waits on §7.2.

All of them share one evaluation function with different weights. Putting an LLM in the loop could come later. Not for v1.

## 10a. Architecture and engine

**Ruled:**
- **An immersive 3D world** that you can walk around in at street level *and* manage from a bird's-eye RTS view. Manor Lords is the reference.
- **One architecture for both modes.** Single-player is multiplayer against a server on `127.0.0.1`. The split between client and server is invisible to someone playing alone.

**[PROPOSED]** Layout:

```
 sim-core (Rust library, headless, deterministic, 1 tick = 10 game-seconds = 7.2 ticks/real second)
   ├── server   : authoritative game server (multiplayer on a hosted server; single-player on 127.0.0.1, started and stopped by the client)
   └── sim-cli  : balance harness, 1,000 AI-vs-AI games, no rendering
 client (3D engine) ──protobuf over WebSocket/QUIC──► server
```

- **The server only sends what the current view needs.** Walk mode gets every individual pedestrian within about 100 m. RTS mode gets crowd density and money flows for the whole map. One protocol, two levels of detail.
- **Single-player:** the client starts the server binary on a random localhost port with a one-time token. The code path is identical to multiplayer. Pause is a host privilege, and saves are server snapshots.
- **Platforms**: **Linux first. If it works on Linux, macOS is fine.** **No Microsoft stack**, which rules out C#/.NET, Unity and Windows-only tooling.
- **Client engine: Godot 4**. MIT licence, run by a non-profit foundation, native on Linux and macOS.
  - **GDScript** for scenes, cameras and UI.
  - **Rust via `godot-rust` (gdext)** for the network client and anything heavy.
  - A shared Rust `protocol` crate defines the messages once, for both server and client.
  - Unreal 5 is the fallback if Godot hits a visual ceiling we can't live with. Its licence is fine for a pet project.
- **World scale**: **1 lot unit = 4 m** by default (configurable), so a 9×9 lot is 36 m across.
- **Multiplayer audience**: **friends connecting remotely** first. Internet and community play is a maybe for later.
  - **[PROPOSED]** v1 multiplayer:
    - The host runs the server.
    - Friends join with an invite code over TLS.
    - The server is authoritative, so clients can only send commands. That's enough cheat protection between friends.
  - Accounts, lobbies and moderation for community play go to the backlog. Nothing in v1 blocks them.

## 10b. Configuration (ruled: YAML)

Every tunable lives in YAML. The server loads it at game start, and a game's save file stores the config it was started with, so a running game never changes under you. The balance harness (`sim-cli`) takes config overrides, so it can sweep a value across a run. **[PROPOSED]** File layout:

```
config/
  game.yaml        # clock, presets, bankruptcy, overdraft, net worth weights
  map.yaml         # 9×9 grid, districts, tiers, sets, land base prices, tube stations + line opening years
  auctions.yaml    # countdown, turn timer, increments
  utilities.yaml   # base rates, nationalisation rules
  businesses.yaml  # every business type: footprint, stock density, hours, goods, utility factors, unlock/obsolete years
  customers.yaml   # population curve, class split by era, budgets, price sensitivity, shopping-list probabilities
  events.yaml      # history + random events, elections
  ai.yaml          # AI personalities and weights
```

Example:

```yaml
# game.yaml
world:
  metres_per_lot_unit: 4
  lot_size: [9, 9]
  max_height: 100
clock:
  real_minutes_per_day: 20
  years_per_day: 5
  start_year: 1930
  end_year: 2050
  trading_hours:
    day:   { open: "07:00", close: "17:00" }
    night: { open: "19:00", close: "23:50" }
    hotel: { open: "23:30", close: "03:00" }
bankruptcy:
  strikes_to_bankrupt: 2
  solvent_at_or_above: 0
  sell_to_city_ratio: 0.5
overdraft:
  start_limit: 10000
  increase_per_step: 2500
  interest_rate: 0.0
net_worth:
  lot_property_uplift: 0.2
leases:
  term_years: 30
```

- **Validation:** a JSON Schema for each file. The server refuses to start on an invalid config and names the bad key, so nothing fails silently.
- **Mods for free:** a different city is a different `map.yaml`.

## 10c. v1 build plan (approved)

**Goal:** play a 1930–1950 game (4 days, 80 minutes) on a grey-box 3×3 map, single-player against one AI, in both camera modes, end to end. Also: the balance harness runs 1,000 headless games.

**Repo:** `dfoulkes/OpenTycoon`, restarted as a monorepo. The 2019 Java stub gets archived to a branch.

```
OpenTycoon/
  Cargo.toml            # Rust workspace
  crates/
    sim-core/           # the game: deterministic, headless, no I/O
    protocol/           # message types shared by server and client
    server/             # WebSocket server; single-player mode binds to 127.0.0.1
    sim-cli/            # balance harness: N games → CSV
    client-ext/         # godot-rust (gdext) extension: network client, interpolation
  client/               # Godot 4 project (GDScript scenes, cameras, UI)
  config/               # YAML defaults + JSON Schemas (§10b)
```

| Milestone | Delivers | Done when |
|---|---|---|
| **M0 Skeleton** | Workspace, Godot project, YAML loading + schema validation, Linux CI | CI builds everything; a bad YAML key fails loudly |
| **M1 Sim core** | Clock, 06:00 settlement, midnight reckoning, bankruptcy, overdraft, 3×3 map (one set per row), auctions, leases, five 1930 businesses (Bakery, Grocery, Clothes Store by day; Bar, Restaurant by night), customers spawning at tube stations and city apartments, class + price shop choice, net worth, one AI (the Grocer) | Unit tests per rule; `sim-cli` plays 1,000 games 1930–1950 and writes CSV |
| **M2 Server** | Protocol, snapshots + per-tick deltas, two levels of detail (walk vs RTS), single-player server started on 127.0.0.1, save/load | A scripted bot client plays a full game over the socket |
| **M3 RTS client** | Grey-box city (boxes for buildings, capsules for walkers), bird's-eye camera, auction UI, shop panel (prices, tomorrow's order), money HUD | You can play the v1 game from the bird's-eye view |
| **M4 Walk mode** | Street-level camera, walker interpolation, switching between camera modes | Walk down Old Kent Road and watch your own shop trade |
| **M5 Friends** | Server container, invite codes, TLS, metrics and an operations runbook | A friend joins from outside the house and finishes a game |

**After v1:** Hotel and tourists, residential (needs its design session), elections and utilities, events, the full 9×9 map, eras past 1950, more AIs, art.

## 11. Notes and risks

- **IP:** "Monopoly" is a Hasbro trademark. The 1930–1980 business table is near-verbatim from the original game's manual. Before anything public, regenerate the business stats from our own formulas and drop "Monopoly" from the name. The sets-and-hotel mechanic is generic enough. (Raised once, already.)
- **Scope:** the thinnest **playable v1** slice is:
  1. clock + settlement + bankruptcy
  2. a 3×3 test map: one set per row, one tier per row
  3. auctions
  4. five 1930 businesses
  5. customer model with class + price
  6. a 1930–1950 game with one AI

  Everything else layers on top.
- **Balance:** the economy needs a **headless simulation harness** from day one. Run 1,000 AI-vs-AI games per preset, then chart median net worth and bankruptcy rate. Tune constants from that, not by feel. Without inflation, the population growth curve is the main dial to tune.

## 12. Decisions

### Ruled

| # | Decision |
|---|---|
| R1 | Trading hours: day 07:00–17:00, night 19:00–23:50. Stock is written off at midnight. |
| R2 | No emergency mid-day restock |
| R3 | £0 at midnight is solvent |
| R4 | Bankrupt player's leases, buildings and stock go to the city. The city runs the businesses at default settings and can't go bust. |
| R5 | Selling up: instant sale back to the city at 50% of true value, or a business auction on its own. Land auctions are land only. Every auction follows the same rules, with no shortcuts. |
| R6 | No inflation. Daily population growth drives the economy. |
| R7 | Overdraft limit £10,000, +£2,500 flat per 5-year step. Bid ceiling = balance + credit limit. |
| R8 | An auction pass counts for one round only. A full round with no new bids closes the auction. |
| R9 | Lease expiry: plain re-auction |
| R10 | Rent, utilities and stock capacity use building volume |
| R11 | Telephone exchange from 1930, upgrades to the Internet in 1995 |
| R12 | Utilities are auctioned like land. Elections decide nationalisation (Labour) and privatisation (Conservative), as in history. |
| R13 | One building = one type (no stacked floors) |
| R14 | Colour sets of 2 or 3 blocks, board-game style. Holding a full set unlocks a Hotel. |
| R15 | E-commerce spend from 1995, taken from the customer's daily budget, with no travel |
| R16 | Kept: desirability shifts, newspaper headlines, tube lines, fit-out, class split by era, reputation, all events, Sir Penny Sugar |
| R17 | All constants configurable |
| R18 | Warning UI deferred to the engine conversation |
| R19 | Net worth = land value + property value + cash + stock, scored at the final midnight before stock is binned. Utilities count only through the cash they save and generate. |
| R20 | Residential is always apartments, never houses. Full design is still to come. |
| R21 | No offices or office jobs |
| R22 | Every block is a 9×9 lot holding several businesses, and each business must touch the lot edge |
| R23 | Minimum opening bid = the district's land base price, set by affluence (Mayfair high, Old Kent Road low) |
| R24 | Utilities aren't a set. A holder's own bills for that utility net to zero, because they're paying themselves. |
| R25 | Shoppers come from residences (apartments) and arrive at tube stations |
| R26 | Sets laid out like a traditional board: 26 sets (one 2-block set at each end, 3-block sets in between), 4 utility blocks, City Hall in the centre |
| R27 | Hotels serve tourists. tourist_pool = total population − housed population. |
| R28 | The city starts the game owning an apartment block in each Low-tier set. Players can buy them or win them at auction. |
| R29 | Hotel mechanics as in §7.3: nightly bed demand, room revenue per guest, souvenir bonus, one Hotel per set |
| R30 | Hotels trade 23:30–03:00, the only business open then. No stock: rooms are capacity set by volume, like apartments. |
| R31 | Net worth is recalculated every midnight, before the write-off |
| R32 | The 06:00 bills can push a player past the overdraft limit. No interest. |
| R33 | Leases can be auctioned but not sold directly |
| R34 | Day length is 20 real minutes, and the year steps +5 every midnight (24-day full game) |
| R35 | No staff costs |
| R36 | E-commerce unlocks in 1995 |
| R37 | Single-player and multiplayer are both in scope |
| R38 | The hotel's 23:30 opening overlaps night trade on purpose. A bed is a tourist's last need: hotel first, otherwise the train home. |
| R39 | Lot value = land value + 20% of every property on the lot (any owner), credited to the leaseholder. Property value is credited to its owner. |
| R41 | Buildings don't wear. Property value = build cost. |
| R42 | A tourist who has finished shopping wanders until 23:30, then takes a hotel room or the train home |
| R43 | Immersive 3D, with a walkable street view and an RTS bird's-eye view (Manor Lords style) |
| R44 | Single-player is multiplayer on 127.0.0.1, with the client/server split hidden from the player |
| R45 | All values configurable in YAML (§10b) |
| R46 | No Microsoft stack. Linux first, macOS fine. |
| R47 | Client: Godot 4 with GDScript plus Rust (godot-rust/gdext) |
| R48 | World scale 4 m per lot unit by default, configurable |
| R49 | Multiplayer for remote friends first; internet/community play maybe later |
| R50 | Flat count = volume ÷ flat size (configurable, default 4 units) |
| R51 | The owner sets rent; demand uses business mechanics. Nicer lot and building attract more, balanced by price. |
| R52 | Rent ideally ≤ 20% of daily allowance, stretching to 50% for high desirability. If the cheapest flat is over 20%, the person commutes or stays a tourist. |
| R53 | New population arrives by train at 05:30 and looks for a flat first |
| R54 | Residents can move at most once every 10 years, under the same affordability rule |
| R55 | Allowance size and desires depend on age group, class and sex |
| R56 | Apartments pay rent and utilities by the volume formulas, with no stock |
| R57 | No height cap by era |
| R58 | Each business type has a configurable `max_height` (≤ the global 100) |
| R59 | Visitors split into commuters (70%, always the train home) and tourists (30%, hotel first). Only tourists feed hotels. |
| R60 | Age bands 18–29, 30–49, 50–64, 65+. Children aren't shoppers. |
| R61 | Population is a cohort model. Births per woman follow history, and children join the arrivals at 18. |
| R62 | Players start with £7,000 cash (configurable) |
| R63 | Shoppers' daily allowances are new money created each morning. No household accounts or wages. |
| R64 | Balance is found by study: ~100 economic models (parameter sets), each tested over 500 games of the full 1930–2050 span, with demand changing by decade |
| R65 | A balanced model: every AI personality wins 10–40% of games (seats rotated), bankruptcy under 15% of player-games, and an idle player finishes below every AI that is still solvent, in every game. Models are ranked by how even the win shares are. |
| R40 | Client/server architecture: the server runs the simulation and clients display it |

### Open

- **a follow-up:** regenerate the business stats from our own formulas.
