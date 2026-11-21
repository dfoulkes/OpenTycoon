# Inspector protocol (v1)

The contract between the server and the Godot client for the game inspector (spec §10d, R88).
The server, the `client-ext` network node and every client scene are built against this page.
Change it in the same PR as the code that changes, and bump `PROTOCOL_VERSION`.

## Transport

- WebSocket, text frames, one JSON object per frame, tagged by `"type"` (snake_case).
- Single player: `client-ext` starts the server in-process on `127.0.0.1:0` (an OS-chosen port)
  and connects to it (R44). Nothing is exposed beyond localhost.
- The server is authoritative (R40). The client only sends the commands below.

## Units

- **Money** is integer pence (the sim's `Money`). The client formats it (`£1,234`).
- **Positions** are metres on the ground plane: `x` east, `z` south, origin at the board's
  north-west corner. One lot unit is `metres_per_lot_unit` (4 m, R48).
- **Rectangles** (`blocks`, `streets`, `river`, `bridges`) are `{x, z, w, d}` in metres with
  `(x, z)` the rectangle's **north-west corner** (its least `x` and `z`), same origin as the board.
- JSON integers stay integers on the wire (money, ticks, counts); `SimLink` hands them to GDScript
  as `int`. Code that receives them as floats (a fixture) should `int()` them.
- **Players** are ids `"p0"`, `"p1"`, … The city is `"city"`. Unowned land is `"unowned"`.

## Client → server

| type | fields | effect |
|---|---|---|
| `hello` | `protocol: u32`, `token: string` | first message; the server replies `welcome`, or `error` and closes (wrong protocol, wrong token, or anything else first) |
| `new_game` | `seed: u64`, `economy: string \| null` (path to an `economy.yaml`, null or `""` = shipped), `players: u8` (default 4) | starts a game; every seat is an AI (the `ai.yaml` roster from the last seat down), the viewer is a spectator; the server replies `snapshot`. `players` must be within `game.yaml` `players` and the roster size (2–5 shipped), else `error` |
| `set_speed` | `speed: u8` | `0` paused, `1`–`4` the speed steps in `config/server.yaml`; anything else is an `error`. The next `delta` carries the new speed, even when paused |
| `leave` | — | ends the game; the server closes the connection |

The token is `server.yaml` `token.length` hex characters; `SimLink` makes a fresh one for each
embedded server, which only accepts that one.

## Server → client

### `welcome`
`protocol: u32`

### `snapshot` (once, after `new_game`)

```json
{
  "type": "snapshot",
  "tick": 0,
  "clock": { "year": 1930, "day": 0, "time": "06:00", "speed": 1 },
  "board": {
    "cols": 9, "rows": 9, "width_m": 0, "depth_m": 0,
    "blocks": [
      { "key": "old_kent_road", "name": "Old Kent Road", "district": "…", "set": "…",
        "col": 0, "row": 0, "x": 0.0, "z": 0.0, "w": 0.0, "d": 0.0, "buildable": true }
    ],
    "river":   [ { "x": 0.0, "z": 0.0, "w": 0.0, "d": 0.0 } ],
    "bridges": [ { "x": 0.0, "z": 0.0, "w": 0.0, "d": 0.0 } ],
    "streets": [ { "x": 0.0, "z": 0.0, "w": 0.0, "d": 0.0 } ]
  },
  "players": [ { "id": "p0", "name": "The Grocer", "cash": 0, "net_worth": 0, "bankrupt": false,
                 "costs_yesterday": 0, "profit_yesterday": 0 } ],
  "lots":    [ "<lot>, one per block" ],
  "auction": null,
  "bids":    [ "<bid>, recent history, newest last" ],
  "walkers": [ 0.0, 0.0 ]
}
```

The board: `cols`/`rows` count blocks; `width_m`/`depth_m` are the whole board including the road
ring. Each block's rectangle is its lot (the buildable land inside the roads). `district` is the
display name of the block's set and `set` its id; both are `""` for a block outside any set
(utilities, City Hall). `streets` are one rectangle per road segment between junction rows (no
cell drawn twice); `river` is the river's road row away from the bridges; `bridges` are the
crossing cells (west side of their column, R85). Streets never overlap river or bridges.
`snapshot.clock.time` is the real game time (a new game starts at `00:00`, tick 0).

### `delta`
Same fields as `snapshot` except `board`. `lots` and `players` hold **only what changed**
(replace by `key` / `id`). `bids` holds **only new** events. `auction` and `walkers` are always
the full current value.

The server runs ticks at the current speed's rate and sends a delta at most
`server.yaml` `deltas_per_second` times a second (10 shipped); each delta covers every tick since
the last, so `tick` can jump by more than one. No deltas while paused, except the one that
reports a speed change.

### `game_over`
`winner: string` (`"p2"`, `"tie"` or `"none"`), `players: [<player>]`. Sent once, after the
final whistle; the connection stays open until the client leaves.

### `error`
`message: string`

## Shapes

**player**
`{ "id": "p0", "name": "The Grocer", "cash": 0, "net_worth": 0, "bankrupt": false, "costs_yesterday": 0, "profit_yesterday": 0 }`

- `name` is the display name of the AI personality playing the seat.
- `net_worth` is the economy's board at the last midnight (cash before the first midnight).
- `costs_yesterday`: the daily running cost (land rent, utilities, standing order at wholesale)
  of every building the player owns: what 06:00 bills them for those buildings. It is computed
  from the buildings, not read back from the settlement, so it leaves out land rent the player
  *collects* as a leaseholder and any interest.
- `profit_yesterday`: over the same buildings, the last completed day's takings plus residents'
  rent, less `costs_yesterday`.

**lot**
```json
{ "key": "mayfair", "owner": "p1", "land_value": 0,
  "lease": { "years_left": 12 },
  "building": { "kind": "shop", "name": "…", "floors": 2, "flats": 0 },
  "business": { "name": "…", "kind": "…", "city_run": false, "sold_today": 0, "capacity": 0,
                "goods": [ { "good": "bread", "cost": 0, "price": 0, "sold_yesterday": 0,
                             "order": 0, "stock": 0, "sold_today": 0 } ] },
  "buildings": 1,
  "takings_per_day": 0, "costs_per_day": 0, "profit_per_day": 0, "rent_per_day": 0,
  "open_for_bids": false, "last_sale_price": null }
```
- `lease`, `building` and `business` are `null` when absent.
- `owner`: the leaseholder (`"p1"`, or `"city"` after a bankruptcy, R4), the holder for a utility
  block, `"city"` for City Hall, else `"unowned"`. The city's starting apartments stand on unowned
  land: the lot is `"unowned"` and its `business.city_run` is `true`.
- `land_value`: the lease's value now under `economy.yaml` `land.value_method` (§3); for unleased
  land, the current market price (the base price under `book_decay`).
- A lot can hold several buildings (up to eight shops round the edge, R22). `building` and
  `business` describe the **largest** (by volume, lowest id on a tie) and `buildings` counts them
  all; the money fields add up **every** building on the lot.
- `building.kind`: `"shop"`, `"apartments"`, `"hotel"`, `"online"` or `"infrastructure"`.
  `floors` is the height in lot units (the sim has no storeys, R13; draw one unit as one floor).
  `flats` is non-zero for apartments only.
- `business.kind` is the type id from `businesses.yaml`, `name` its display name.
  `sold_today` is units sold since midnight over every good; `capacity` is the units it can stock,
  `capacity_per_good × goods`.
- `business.goods` is the shop table, one row per good it sells, in id order (empty for flats):
  `cost` the wholesale price per unit, `price` the selling price per unit, `sold_yesterday` units
  over the last completed day, `order` the standing order (units delivered each 06:00; it locks
  into tomorrow's order at midnight, R2), `stock` units on hand now, `sold_today` units since
  midnight.
- `takings_per_day`: sales and room nights over the **last completed game day** (0 on day 0).
- `costs_per_day`: the daily running cost (land rent, utilities, standing order), live.
- `rent_per_day`: what the lot's residents pay their landlord each 06:00 (R51), live.
- `profit_per_day = takings_per_day + rent_per_day − costs_per_day`.
- `open_for_bids`: the lot is the live auction's, or is leasable land with no holder.
- `last_sale_price`: the price of the last auction it was sold at, or `null`.

**auction** (the live one, or `null`)
```json
{ "block": "mayfair", "item": "land", "high_bid": 0, "high_bidder": "p1",
  "seconds_left": 0, "round": 1, "turn": "p2", "opener": "p1", "opening_bid": 0,
  "seats": [ { "player": "p0", "status": "waiting", "last_bid": null } ] }
```
- `item`: `"land"`, `"business"` or `"utility"`; `block` is the block it stands on.
- `high_bid` is the reserve until someone bids; `high_bidder` is `null` until then.
- `seconds_left`: real seconds at 1× until bidding opens (countdown) or the turn runs out.
- `round`: `0` in the countdown, then `1`, `2`, …: a new round starts each time the turn comes back
  round the table.
- `turn`: whose go it is (`null` in the countdown).
- `opener`: the player who opened it with a bid, else the seller (`"city"` for the game's own
  sales, e.g. an expired lease, R9). `opening_bid`: that bid, or the reserve.
- `seats`: one per player, in seat order. `status` is `"to_act"` (their turn), `"raised"` (holds
  the high bid), `"passed"` (passed this round; a pass counts for one round only, R8),
  `"waiting"` (still in), or `"out"` (bankrupt, or the seller). `last_bid` is their last bid in
  this auction, or `null`. The auction house keeps only the leader and the high bid, so passes
  and last bids are the server's record of the room since it opened.

**bid**
`{ "tick": 0, "block": "mayfair", "player": "p1" | null, "amount": 0, "kind": "open" | "raise" | "pass" | "won" | "unsold" }`

- `open`: `player` is the opener (`null` for the game's own sales), `amount` the opening bid or
  reserve. `raise`: the new high bidder and bid. `pass`: who passed (or let the timer run out)
  and the standing high bid. `won`: the winner and price. `unsold`: nobody bid (`player` null,
  `amount` the reserve), or the winner could not pay (`player` the defaulter, `amount` the bid).
- The server reads these from the auction house between ticks, so two things in the same tick
  show as one (a raise wins over a pass).
- `snapshot.bids` holds the last `server.yaml` `recent_bids` (50) events.

**walkers**: a flat array `[x0, z0, x1, z1, …]` in metres (the centre of each walker's street
cell), at most `max_walkers` (`config/server.yaml`). No ids in v1; the client draws them with one
MultiMesh.

## Godot side: `SimLink` (client-ext)

A `Node` the game scene owns. It polls its socket in `_process`.

| Kind | Name | Notes |
|---|---|---|
| method | `start_single_player(config_dir: String, economy_path: String, seed: int)` | starts the embedded server, connects, sends `hello` then `new_game` (4 players); `economy_path` "" = shipped. Paths may be `res://`/`user://` or plain (e.g. `"res://../config"` from the repo's `client/`) |
| method | `set_speed(speed: int)` | |
| method | `leave()` | `disconnected` follows when the server closes |
| method | `is_connected_to_server() -> bool` | |
| signal | `snapshot_received(data: Dictionary)` | the parsed `snapshot` |
| signal | `delta_received(data: Dictionary)` | the parsed `delta` |
| signal | `game_over(data: Dictionary)` | |
| signal | `link_error(message: String)` | a server `error`, or the link failing (start-up errors included) |
| signal | `disconnected()` | |

Scenes never talk to the socket: they expose `apply_snapshot(data)` / `apply_delta(data)` and are
tested with dictionaries. `client/scripts/net/fake_sim_link.gd` has the same methods and signals
and replays a fixture, so the client runs without the server.

## Input actions (`client/project.godot`)

| Action | Default |
|---|---|
| `cam_pan_left` / `cam_pan_right` / `cam_pan_up` / `cam_pan_down` | A / D / W / S, arrow keys |
| `cam_rotate_left` / `cam_rotate_right` | Q / E |
| `cam_zoom_in` / `cam_zoom_out` | mouse wheel up / down |
| `cam_drag_rotate` | middle mouse |
| `select` | left mouse |
| `pause_toggle` | Space |
| `speed_1` … `speed_4` | 1 … 4 |
| `menu` | Escape |
