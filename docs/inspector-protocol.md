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
- **Players** are ids `"p0"`, `"p1"`, … The city is `"city"`. Unowned land is `"unowned"`.

## Client → server

| type | fields | effect |
|---|---|---|
| `hello` | `protocol: u32`, `token: string` | first message; the server replies `welcome` or `error` |
| `new_game` | `seed: u64`, `economy: string \| null` (path to an `economy.yaml`, null = shipped), `players: u8` (default 4) | starts a game; every seat is an AI, the viewer is a spectator; the server replies `snapshot` |
| `set_speed` | `speed: u8` | `0` paused, `1`–`4` the speed steps in `config/server.yaml` |
| `leave` | — | ends the game; the server closes the connection |

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
  "players": [ { "id": "p0", "name": "The Grocer", "cash": 0, "net_worth": 0, "bankrupt": false } ],
  "lots":    [ "<lot>, one per block" ],
  "auction": null,
  "bids":    [ "<bid>, recent history, newest last" ],
  "walkers": [ 0.0, 0.0 ]
}
```

### `delta` (each sim step the server sends)
Same fields as `snapshot` except `board`. `lots` and `players` hold **only what changed**
(replace by `key` / `id`). `bids` holds **only new** events. `auction` and `walkers` are always
the full current value.

### `game_over`
`winner: string` (`"p2"`, `"tie"` or `"none"`), `players: [<player>]`

### `error`
`message: string`

## Shapes

**lot**
```json
{ "key": "mayfair", "owner": "p1", "land_value": 0,
  "lease": { "years_left": 12 },
  "building": { "kind": "shop", "name": "…", "floors": 2, "flats": 0 },
  "business": { "name": "…", "kind": "…", "city_run": false },
  "takings_per_day": 0, "costs_per_day": 0, "profit_per_day": 0, "rent_per_day": 0,
  "open_for_bids": false, "last_sale_price": null }
```
`lease`, `building` and `business` are `null` when absent. `profit_per_day = takings − costs`
over the last completed game day. City-owned lots have `"owner": "city"`.

**auction** (the live one, or `null`)
`{ "block": "mayfair", "item": "land", "high_bid": 0, "high_bidder": "p1" | null, "seconds_left": 0, "round": 1 }`

**bid**
`{ "tick": 0, "block": "mayfair", "player": "p1" | null, "amount": 0, "kind": "open" | "raise" | "pass" | "won" | "unsold" }`

**walkers**: a flat array `[x0, z0, x1, z1, …]` in metres, at most `max_walkers`
(`config/server.yaml`). No ids in v1; the client draws them with one MultiMesh.

## Godot side: `SimLink` (client-ext)

A `Node` the game scene owns. It polls its socket in `_process`.

| Kind | Name | Notes |
|---|---|---|
| method | `start_single_player(config_dir: String, economy_path: String, seed: int)` | starts the embedded server, connects, sends `hello` then `new_game`; `economy_path` "" = shipped |
| method | `set_speed(speed: int)` | |
| method | `leave()` | |
| signal | `snapshot_received(data: Dictionary)` | the parsed `snapshot` |
| signal | `delta_received(data: Dictionary)` | the parsed `delta` |
| signal | `game_over(data: Dictionary)` | |
| signal | `link_error(message: String)` | |
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
