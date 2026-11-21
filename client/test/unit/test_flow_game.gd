extends GutTest
## The game controller: forwards link data to the child scenes, drives time, the in-game menu
## and the quit paths. Every child is a spy from test/fixtures.

const GameScene := preload("res://scenes/game/game.tscn")
const SpyLink := preload("res://test/fixtures/spy_link.gd")
const SpyWorld := preload("res://test/fixtures/spy_world.gd")
const SpyUi := preload("res://test/fixtures/spy_ui.gd")
const FIXTURE := "res://test/fixtures/snapshot_small.json"

var game: Node
var link: Node
var world: Node
var hud: Control
var card: Control
var feed: Control
var exits: Array


func before_each() -> void:
	game = GameScene.instantiate()
	link = SpyLink.new()
	world = SpyWorld.new()
	hud = SpyUi.new()
	card = SpyUi.new()
	feed = SpyUi.new()
	exits = []
	game.exit_handler = func(to_menu: bool) -> void: exits.append(to_menu)
	game.game_seed = 77
	game.economy_path = "/tmp/econ.yaml"
	game.link_mode = "real"
	game.wire(link, world, hud, card, feed)
	add_child_autofree(game)


func _snapshot() -> Dictionary:
	return JSON.parse_string(FileAccess.get_file_as_string(FIXTURE))


func _press(action: String) -> void:
	var ev := InputEventAction.new()
	ev.action = action
	ev.pressed = true
	game._unhandled_input(ev)


func test_ready_starts_single_player_with_seed_and_economy() -> void:
	var starts: Array = link.called("start_single_player")
	assert_eq(starts.size(), 1)
	assert_eq(starts[0][1], "/tmp/econ.yaml")
	assert_eq(starts[0][2], 77)


func test_snapshot_forwards_to_world_hud_and_feed() -> void:
	var snap := _snapshot()
	link.snapshot_received.emit(snap)
	assert_eq(world.called("apply_snapshot").size(), 1)
	assert_eq(hud.called("update_clock")[0][0], snap["clock"])
	assert_eq(hud.called("update_players")[0][0].map(func(p: Dictionary) -> String: return p["id"]), ["p0", "p1"])
	var bids: Array = feed.called("add_bids")
	assert_eq(bids.size(), 1)
	assert_eq(bids[0][0].size(), 5)
	assert_eq(bids[0][2].size(), 9, "blocks by key")
	assert_eq(game.lots.size(), 9)


func test_delta_merges_state_and_forwards() -> void:
	link.snapshot_received.emit(_snapshot())
	var delta := {
		"type": "delta", "tick": 57, "clock": {"year": 1930, "day": 3, "time": "10:50", "speed": 1},
		"players": [{"id": "p0", "name": "The Grocer", "cash": 5, "net_worth": 6, "bankrupt": false}],
		"lots": [{"key": "sloane_street", "owner": "p0", "land_value": 1}],
		"auction": null, "bids": [{"tick": 57, "block": "sloane_street", "player": "p0",
			"amount": 1, "kind": "won"}], "walkers": [],
	}
	link.delta_received.emit(delta)
	assert_eq(world.called("apply_delta").size(), 1)
	assert_eq(game.lots["sloane_street"]["owner"], "p0")
	assert_eq(game.players["p0"]["cash"], 5)
	assert_eq(game.players["p1"]["name"], "The Landlord", "unchanged players kept")
	assert_eq(feed.called("add_bids").size(), 2)
	assert_eq(hud.called("update_clock").size(), 2)


func test_lot_click_shows_card_and_highlights() -> void:
	link.snapshot_received.emit(_snapshot())
	world.lot_clicked.emit("borough_high_street")
	var shown: Array = card.called("show_lot")
	assert_eq(shown.size(), 1)
	assert_eq(shown[0][0]["owner"], "city")
	assert_eq(shown[0][2]["name"], "Borough High Street")
	assert_eq(world.called("highlight_lot"), [["borough_high_street"]])


func test_delta_refreshes_selected_lot_card() -> void:
	link.snapshot_received.emit(_snapshot())
	world.lot_clicked.emit("sloane_street")
	link.delta_received.emit({"lots": [{"key": "sloane_street", "owner": "p1"}], "bids": []})
	var upd: Array = card.called("update_lot")
	assert_eq(upd.size(), 1)
	assert_eq(upd[0][0]["owner"], "p1")


func test_delta_for_other_lot_does_not_refresh_card() -> void:
	link.snapshot_received.emit(_snapshot())
	world.lot_clicked.emit("sloane_street")
	link.delta_received.emit({"lots": [{"key": "upper_street", "owner": "p0"}], "bids": []})
	assert_eq(card.called("update_lot").size(), 0)


func test_speed_keys_call_link() -> void:
	_press("speed_3")
	_press("speed_1")
	_press("speed_4")
	_press("speed_2")
	assert_eq(link.called("set_speed"), [[3], [1], [4], [2]])


func test_pause_toggle_pauses_then_resumes_last_speed() -> void:
	_press("speed_3")
	_press("pause_toggle")
	_press("pause_toggle")
	assert_eq(link.called("set_speed"), [[3], [0], [3]])


func test_time_control_buttons_call_link() -> void:
	var tc: Control = game.get_node("%TimeControls")
	tc.get_node("%Speed2Button").pressed.emit()
	tc.get_node("%PauseButton").pressed.emit()
	assert_eq(link.called("set_speed"), [[2], [0]])


func test_menu_action_opens_menu_and_pauses() -> void:
	_press("speed_2")
	_press("menu")
	var menu: Control = game.get_node("%InGameMenu")
	assert_true(menu.visible)
	assert_eq(link.called("set_speed").back(), [0])


func test_continue_restores_previous_speed() -> void:
	_press("speed_4")
	_press("menu")
	game.get_node("%InGameMenu").get_node("%ContinueButton").pressed.emit()
	assert_false(game.get_node("%InGameMenu").visible)
	assert_eq(link.called("set_speed"), [[4], [0], [4]])


func test_menu_action_again_acts_as_continue() -> void:
	_press("speed_2")
	_press("menu")
	_press("menu")
	assert_eq(link.called("set_speed"), [[2], [0], [2]])


func test_speed_keys_ignored_while_menu_open() -> void:
	_press("menu")
	_press("speed_3")
	assert_eq(link.called("set_speed"), [[0]])


func test_in_game_settings_opens_and_back_returns() -> void:
	_press("menu")
	var menu: Control = game.get_node("%InGameMenu")
	menu.get_node("%SettingsButton").pressed.emit()
	assert_true(menu.get_node("%SettingsMenu").visible)
	assert_false(menu.get_node("%Buttons").visible)
	menu.get_node("%SettingsMenu").get_node("%BackButton").pressed.emit()
	assert_true(menu.get_node("%Buttons").visible)


func test_quit_to_main_menu_leaves_then_exits_to_menu() -> void:
	_press("menu")
	game.get_node("%InGameMenu").get_node("%QuitToMenuButton").pressed.emit()
	assert_eq(link.called("leave").size(), 1)
	assert_eq(exits, [true])


func test_quit_leaves_then_exits_app() -> void:
	_press("menu")
	game.get_node("%InGameMenu").get_node("%QuitButton").pressed.emit()
	assert_eq(link.called("leave").size(), 1)
	assert_eq(exits, [false])
