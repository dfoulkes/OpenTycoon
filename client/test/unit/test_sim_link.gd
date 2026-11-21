extends GutTest
## SimLink (crates/client-ext) against the real server, in-process on 127.0.0.1 (R44, R88):
## start a single-player game, receive the snapshot and a delta as Dictionaries, then leave.

const CONFIG_DIR := "res://../config"

var _snapshot: Dictionary = {}
var _delta: Dictionary = {}
var _errors: Array = []


func _on_snapshot(data: Dictionary) -> void:
	_snapshot = data


func _on_delta(data: Dictionary) -> void:
	_delta = data


func _on_error(message: String) -> void:
	_errors.append(message)


func test_sim_link_is_registered() -> void:
	assert_true(ClassDB.class_exists("SimLink"), "build it with: cargo build -p client-ext")


func test_a_single_player_game_streams_a_snapshot_then_deltas() -> void:
	var link: Node = ClassDB.instantiate("SimLink")
	add_child_autofree(link)
	link.snapshot_received.connect(_on_snapshot)
	link.delta_received.connect(_on_delta)
	link.link_error.connect(_on_error)
	link.start_single_player(CONFIG_DIR, "", 42)
	await wait_for_signal(link.snapshot_received, 30, "snapshot")
	assert_eq(_errors, [], "no link errors")
	assert_eq(_snapshot.get("type"), "snapshot")
	assert_eq(_snapshot["lots"].size(), 81, "one lot per block of the 9x9 board")
	assert_eq(_snapshot["players"].size(), 4)
	assert_eq(typeof(_snapshot["players"][0]["cash"]), TYPE_INT, "money is integer pence")
	assert_eq(_snapshot["board"]["cols"], 9)

	link.set_speed(4)
	# The first delta may only report the speed change (still at tick 0 on a slow
	# machine); wait for one where the sim has stepped.
	await wait_until(func() -> bool: return int(_delta.get("tick", 0)) > 0, 20, "a delta past tick 0")
	assert_eq(_delta.get("type"), "delta")
	assert_gt(int(_delta.get("tick", 0)), 0)

	link.leave()
	await wait_for_signal(link.disconnected, 10, "disconnected")
	assert_false(link.is_connected_to_server())
