extends Node
## Stand-in for the Rust `SimLink` with the same methods and signals (docs/inspector-protocol.md).
## Replays a fixture snapshot, then makes up plausible deltas on a Timer that honours the speed.
## Single player only (R88). No server, no socket.

signal snapshot_received(data: Dictionary)
signal delta_received(data: Dictionary)
signal game_over(data: Dictionary)
signal link_error(message: String)
signal disconnected

const FIXTURE := "res://test/fixtures/snapshot_small.json"
const SECONDS_PER_STEP := 1.0  # at speed 1
const MINUTES_PER_STEP := 10

var _rng := RandomNumberGenerator.new()
var _timer := Timer.new()
var _state := {}
var _speed := 1


func _init() -> void:
	_timer.timeout.connect(step)
	add_child(_timer)


func start_single_player(_config_dir: String, _economy_path: String, game_seed: int) -> void:
	_rng.seed = game_seed
	var parsed: Variant = JSON.parse_string(FileAccess.get_file_as_string(FIXTURE))
	if not parsed is Dictionary:
		link_error.emit("fake link: cannot read %s" % FIXTURE)
		return
	_state = parsed
	snapshot_received.emit.call_deferred(_state.duplicate(true))
	set_speed(int(_state["clock"]["speed"]))


func set_speed(speed: int) -> void:
	_speed = clampi(speed, 0, 4)
	if _state.has("clock"):
		_state["clock"]["speed"] = _speed
	if _speed == 0 or _state.is_empty():
		_timer.stop()
		return
	_timer.wait_time = SECONDS_PER_STEP / _speed
	if is_inside_tree():
		_timer.start()


func leave() -> void:
	_timer.stop()
	_state = {}
	disconnected.emit()


func timer() -> Timer:
	return _timer


## Advances one sim step and emits the delta. Public so tests can step without waiting.
func step() -> void:
	if _state.is_empty():
		return
	_state["tick"] = int(_state["tick"]) + 1
	_advance_clock(_state["clock"])
	var delta := {
		"type": "delta", "tick": _state["tick"], "clock": _state["clock"].duplicate(),
		"players": [], "lots": [], "bids": [], "auction": null, "walkers": [],
	}
	_trade(delta)
	_run_auction(delta)
	_move_walkers()
	delta["auction"] = _state["auction"].duplicate() if _state["auction"] != null else null
	delta["walkers"] = _state["walkers"].duplicate()
	delta_received.emit(delta)


func _advance_clock(clock: Dictionary) -> void:
	var parts: PackedStringArray = String(clock["time"]).split(":")
	var minutes := int(parts[0]) * 60 + int(parts[1]) + MINUTES_PER_STEP
	if minutes >= 24 * 60:
		minutes -= 24 * 60
		clock["day"] = int(clock["day"]) + 1
	clock["time"] = "%02d:%02d" % [floori(minutes / 60.0), minutes % 60]


## One trading lot's takings wobble; its owner's cash follows.
func _trade(delta: Dictionary) -> void:
	var lots: Array = _state["lots"]
	var lot: Dictionary = lots[_rng.randi_range(0, lots.size() - 1)]
	if lot["business"] == null:
		return
	lot["takings_per_day"] = maxi(0, int(lot["takings_per_day"]) + _rng.randi_range(-300, 300))
	lot["profit_per_day"] = int(lot["takings_per_day"]) - int(lot["costs_per_day"])
	delta["lots"].append(lot.duplicate(true))
	var owner := _player(String(lot["owner"]))
	if not owner.is_empty():
		var share := floori(int(lot["profit_per_day"]) / 10.0)
		owner["cash"] = int(owner["cash"]) + share
		owner["net_worth"] = int(owner["net_worth"]) + share
		delta["players"].append(owner.duplicate())


func _run_auction(delta: Dictionary) -> void:
	var auction: Variant = _state["auction"]
	if auction == null:
		_open_auction(delta)
		return
	auction["seconds_left"] = int(auction["seconds_left"]) - 1
	if auction["seconds_left"] > 0:
		if _rng.randf() < 0.4:
			_raise(auction, delta)
		return
	var winner: Variant = auction["high_bidder"]
	var lot := _lot(String(auction["block"]))
	lot["open_for_bids"] = false
	if winner != null:
		lot["owner"] = winner
		lot["last_sale_price"] = auction["high_bid"]
		lot["lease"] = {"years_left": 25}
	delta["lots"].append(lot.duplicate(true))
	delta["bids"].append(_bid(auction["block"], winner, auction["high_bid"], "won" if winner != null else "unsold"))
	_state["auction"] = null


func _open_auction(delta: Dictionary) -> void:
	var free: Array = []
	for lot: Dictionary in _state["lots"]:
		if lot["owner"] == "unowned":
			free.append(lot)
	if free.is_empty() or _rng.randf() > 0.2:
		return
	var lot: Dictionary = free[_rng.randi_range(0, free.size() - 1)]
	lot["open_for_bids"] = true
	var bidder := "p%d" % _rng.randi_range(0, _state["players"].size() - 1)
	_state["auction"] = {"block": lot["key"], "item": "land", "high_bid": lot["land_value"],
		"high_bidder": bidder, "seconds_left": 15, "round": 1}
	delta["lots"].append(lot.duplicate(true))
	delta["bids"].append(_bid(lot["key"], bidder, lot["land_value"], "open"))


func _raise(auction: Dictionary, delta: Dictionary) -> void:
	var bidder := "p0" if auction["high_bidder"] == "p1" else "p1"
	auction["high_bid"] = int(auction["high_bid"]) + 2000
	auction["high_bidder"] = bidder
	auction["round"] = int(auction["round"]) + 1
	auction["seconds_left"] = maxi(int(auction["seconds_left"]), 6)
	delta["bids"].append(_bid(auction["block"], bidder, auction["high_bid"], "raise"))


## Walkers on a north-south street step along z, everyone else along x, so they stay on streets.
func _move_walkers() -> void:
	var w: Array = _state["walkers"]
	var blocks: Array = _state["board"]["blocks"]
	var street_w := float(blocks[0]["x"])
	var period := street_w + float(blocks[0]["w"])
	var edge := float(_state["board"]["width_m"]) - street_w
	for i in range(0, w.size(), 2):
		var on_ns_street := fmod(float(w[i]), period) < street_w
		var axis := i + 1 if on_ns_street else i
		w[axis] = clampf(float(w[axis]) + _rng.randf_range(-1.5, 1.5), 0.0, edge)


func _bid(block: Variant, player: Variant, amount: Variant, kind: String) -> Dictionary:
	return {"tick": _state["tick"], "block": block, "player": player, "amount": amount, "kind": kind}


func _lot(key: String) -> Dictionary:
	for lot: Dictionary in _state["lots"]:
		if lot["key"] == key:
			return lot
	return {}


func _player(id: String) -> Dictionary:
	for p: Dictionary in _state["players"]:
		if p["id"] == id:
			return p
	return {}
