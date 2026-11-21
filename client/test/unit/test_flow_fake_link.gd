extends GutTest
## The fake SimLink: same surface as the real one, replays the fixture and ticks deltas.

const FakeLink := preload("res://scripts/net/fake_sim_link.gd")

var link: Node
var snaps: Array
var deltas: Array


func before_each() -> void:
	link = FakeLink.new()
	snaps = []
	deltas = []
	link.snapshot_received.connect(func(d: Dictionary) -> void: snaps.append(d))
	link.delta_received.connect(func(d: Dictionary) -> void: deltas.append(d))
	add_child_autofree(link)


func test_has_simlink_surface() -> void:
	for m: String in ["start_single_player", "set_speed", "leave"]:
		assert_true(link.has_method(m), m)
	for s: String in ["snapshot_received", "delta_received", "game_over", "link_error", "disconnected"]:
		assert_true(link.has_signal(s), s)


func test_start_emits_fixture_snapshot() -> void:
	link.start_single_player("", "", 3)
	await wait_physics_frames(2)
	assert_eq(snaps.size(), 1)
	var s: Dictionary = snaps[0]
	assert_eq(s["type"], "snapshot")
	assert_eq(int(s["board"]["cols"]), 3)
	assert_eq(s["board"]["blocks"].size(), 9)
	assert_eq(s["players"].size(), 2)
	assert_not_null(s["auction"])
	assert_true(s["walkers"].size() >= 90, "about 50 walkers")
	var city_run := false
	for lot: Dictionary in s["lots"]:
		if lot["owner"] == "city" and lot["business"] != null and lot["business"]["city_run"]:
			city_run = true
	assert_true(city_run, "a city-owned lot with a city-run business")


func test_step_emits_delta_with_advanced_tick() -> void:
	link.start_single_player("", "", 3)
	await wait_physics_frames(2)
	link.step()
	assert_eq(deltas.size(), 1)
	var d: Dictionary = deltas[0]
	assert_eq(d["type"], "delta")
	assert_eq(int(d["tick"]), int(snaps[0]["tick"]) + 1)
	assert_false(d.has("board"))
	assert_eq(d["walkers"].size(), snaps[0]["walkers"].size())


func test_speed_zero_stops_timer_and_speed_sets_interval() -> void:
	link.start_single_player("", "", 3)
	link.set_speed(0)
	assert_true(link.timer().is_stopped())
	link.set_speed(4)
	assert_false(link.timer().is_stopped())
	var fast: float = link.timer().wait_time
	link.set_speed(1)
	assert_true(link.timer().wait_time > fast)


func test_leave_stops_and_disconnects() -> void:
	watch_signals(link)
	link.start_single_player("", "", 3)
	link.leave()
	assert_true(link.timer().is_stopped())
	assert_signal_emitted(link, "disconnected")
