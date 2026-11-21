extends Node
## Runs the world scene without a server: a generated full board (or the 3×3 test fixture),
## random owners, a cycling auction and a crowd of walkers, fed as protocol dictionaries.
## Open `scenes/world/world_demo.tscn` in the editor and press F6. Click a lot to select it.

const FIXTURE := "res://test/fixtures/world_snapshot.json"
const UNIT_M := 4.0      # metres per lot unit (R48)
const LOT_UNITS := 9     # lot size (R22)
const DISTRICTS: Array[String] = ["East End", "West End", "The City", "South Bank",
	"Kensington", "Camden", "Docklands", "Mayfair", "Lambeth", "Islington"]

## Use the small test fixture instead of the generated 9×9 board.
@export var use_fixture := false
@export var walkers := 20000
@export var delta_seconds := 0.25

var _rng := RandomNumberGenerator.new()
var _snapshot: Dictionary = {}
var _lanes := PackedFloat32Array()   # per walker: lane start x, z, direction x, z, length
var _progress := PackedFloat32Array()
var _since := 0.0
var _auction_tick := 0

@onready var _world: Node3D = $World
@onready var _stats: Label = $Stats


func _ready() -> void:
	_rng.seed = 1930
	_snapshot = _fixture() if use_fixture else _generated_board(9, 9, 5, [1, 3, 4, 5, 7])
	_place_walkers(_snapshot["board"])
	_snapshot["walkers"] = _walker_positions()
	_world.apply_snapshot(_snapshot)
	_world.lot_clicked.connect(_on_lot_clicked)


func _process(delta: float) -> void:
	_since += delta
	if _since >= delta_seconds:
		_since = 0.0
		_world.apply_delta(_next_delta())
	_stats.text = "%d fps · %d walkers · click a lot" % [
		Engine.get_frames_per_second(), _world.walker_count()]


func _on_lot_clicked(key: String) -> void:
	_world.highlight_lot(key)


func _next_delta() -> Dictionary:
	for i in _progress.size():
		_progress[i] = fmod(_progress[i] + 1.4 * delta_seconds, _lanes[i * 5 + 4])
	_auction_tick += 1
	var lots: Array = _snapshot["lots"]
	var changed: Array = []
	if _auction_tick % 8 == 0:
		var lot: Dictionary = lots[_rng.randi() % lots.size()]
		lot["owner"] = ["p0", "p1", "p2", "p3", "city"][_rng.randi() % 5]
		lot["building"] = { "kind": "shop", "name": "Shop", "floors": _rng.randi_range(1, 12), "flats": 0 }
		changed.append(lot)
	var block: Dictionary = _snapshot["board"]["blocks"][int(_auction_tick / 40.0) % lots.size()]
	return {
		"lots": changed,
		"auction": { "block": block["key"], "item": "land", "high_bid": 250_000 * (_auction_tick % 40),
			"high_bidder": "p%d" % (_auction_tick % 4), "seconds_left": 40 - _auction_tick % 40, "round": 1 },
		"walkers": _walker_positions(),
	}


func _walker_positions() -> PackedFloat32Array:
	var flat := PackedFloat32Array()
	flat.resize(_progress.size() * 2)
	for i in _progress.size():
		var lane := i * 5
		flat[i * 2] = _lanes[lane] + _lanes[lane + 2] * _progress[i]
		flat[i * 2 + 1] = _lanes[lane + 1] + _lanes[lane + 3] * _progress[i]
	return flat


## Puts each walker on a random street, walking along it.
func _place_walkers(board: Dictionary) -> void:
	var streets: Array = board.get("streets", [])
	_lanes.resize(walkers * 5)
	_progress.resize(walkers)
	for i in walkers:
		var street: Dictionary = streets[_rng.randi() % streets.size()]
		var w := float(street["w"])
		var d := float(street["d"])
		var along_x := w >= d
		var lane := i * 5
		_lanes[lane] = float(street["x"]) + (0.0 if along_x else _rng.randf() * w)
		_lanes[lane + 1] = float(street["z"]) + (_rng.randf() * d if along_x else 0.0)
		_lanes[lane + 2] = 1.0 if along_x else 0.0
		_lanes[lane + 3] = 0.0 if along_x else 1.0
		_lanes[lane + 4] = maxf(w, d)
		_progress[i] = _rng.randf() * maxf(w, d)


func _fixture() -> Dictionary:
	return JSON.parse_string(FileAccess.get_file_as_string(FIXTURE))


## A board laid out like the server's: a one-unit road ring around every 9×9 lot, the river
## on the road row south of `river_row`, bridges on the west side of their columns (R85).
func _generated_board(cols: int, rows: int, river_row: int, bridges: Array) -> Dictionary:
	var pitch := (LOT_UNITS + 1) * UNIT_M
	var size := Vector2(cols * pitch + UNIT_M, rows * pitch + UNIT_M)
	var river_z := (river_row + 1) * pitch
	var city_hall := Vector2i(int(cols / 2.0), int(rows / 2.0))
	var blocks: Array = []
	var lots: Array = []
	for row in rows:
		for col in cols:
			var key := "block_%d_%d" % [col, row]
			var centre := Vector2i(col, row) == city_hall
			blocks.append({ "key": key, "name": key.capitalize(), "set": "set_%d" % int((row * cols + col) / 3.0),
				"district": DISTRICTS[(row * cols + col) % DISTRICTS.size()], "col": col, "row": row,
				"x": col * pitch + UNIT_M, "z": row * pitch + UNIT_M, "w": LOT_UNITS * UNIT_M,
				"d": LOT_UNITS * UNIT_M, "buildable": not centre })
			lots.append(_random_lot(key, centre))
	var streets: Array = []
	for j in rows + 1:
		if j * pitch != river_z:
			streets.append({ "x": 0.0, "z": j * pitch, "w": size.x, "d": UNIT_M })
	for i in cols + 1:  # north-south roads stop at the river; bridges carry some across
		streets.append({ "x": i * pitch, "z": 0.0, "w": UNIT_M, "d": river_z })
		streets.append({ "x": i * pitch, "z": river_z + UNIT_M, "w": UNIT_M, "d": size.y - river_z - UNIT_M })
	var bridge_rects: Array = []
	for c: int in bridges:
		bridge_rects.append({ "x": c * pitch, "z": river_z, "w": UNIT_M, "d": UNIT_M })
	return {
		"type": "snapshot", "tick": 0,
		"board": { "cols": cols, "rows": rows, "width_m": size.x, "depth_m": size.y, "blocks": blocks,
			"river": [{ "x": 0.0, "z": river_z, "w": size.x, "d": UNIT_M }],
			"bridges": bridge_rects, "streets": streets },
		"lots": lots, "auction": null, "bids": [], "walkers": [],
	}


func _random_lot(key: String, city_hall: bool) -> Dictionary:
	var roll := _rng.randf()
	var holder := "city" if city_hall or roll < 0.1 else ("unowned" if roll < 0.45 else "p%d" % _rng.randi_range(0, 3))
	var building: Variant = null
	if holder != "unowned":
		building = { "kind": "shop", "name": "Shop", "floors": _rng.randi_range(1, 10), "flats": 0 }
	return { "key": key, "owner": holder, "building": building, "lease": null, "business": null }
