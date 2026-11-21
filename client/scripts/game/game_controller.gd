extends Node
## The game scene. Owns the sim link, keeps the current state (lots and blocks by key, players by
## id) and hands it to the world, HUD, lot card and bid feed. Also time controls, the in-game menu
## and the quit paths. Child scenes built elsewhere are optional: missing ones are skipped.

const MAIN_SCENE := "res://scenes/main.tscn"
const WORLD_SCENE := "res://scenes/world/world.tscn"
const HUD_SCENE := "res://scenes/ui/hud.tscn"
const LOT_CARD_SCENE := "res://scenes/ui/lot_card.tscn"
const BID_FEED_SCENE := "res://scenes/ui/bid_feed.tscn"
const FAKE_LINK := "res://scripts/net/fake_sim_link.gd"
const THEME_PATH := "res://theme/inspector_theme.tres"

var economy_path := ""
var game_seed := 0
## Called with `true` for Quit to Main Menu, `false` for Quit. Overridden in tests.
var exit_handler: Callable = _default_exit

var link: Node
var world: Node
var hud: Node
var lot_card: Node
var bid_feed: Node

var lots := {}
var players := {}
## The live auction from the last snapshot or delta, or null.
var auction: Variant = null
var blocks := {}
var selected_lot := ""
var speed := 1

var _resume_speed := 1
var _menu_speed := 1
var _wired := false
var _left := false

@onready var _menu: Control = %InGameMenu
@onready var _time: Control = %TimeControls
@onready var _status: Label = %Status


## Injects the link and child scenes (tests); call before the node enters the tree.
func wire(p_link: Node, p_world: Node, p_hud: Node, p_card: Node, p_feed: Node) -> void:
	link = p_link
	world = p_world
	hud = p_hud
	lot_card = p_card
	bid_feed = p_feed
	_wired = true


func _ready() -> void:
	if ResourceLoader.exists(THEME_PATH):
		(%UiRoot as Control).theme = load(THEME_PATH)
	if not _wired:
		wire(_make_link(), _optional(WORLD_SCENE), _optional(HUD_SCENE), _optional(LOT_CARD_SCENE), _optional(BID_FEED_SCENE))
	_attach()
	_connect_signals()
	link.start_single_player(_config_dir(), economy_path, game_seed)


## "" when the Rust `SimLink` is registered, else the GDScript fake's path.
func link_script_path() -> String:
	return "" if ClassDB.class_exists("SimLink") else FAKE_LINK


func on_snapshot(data: Dictionary) -> void:
	blocks = _by(data.get("board", {}).get("blocks", []), "key")
	lots = _by(data.get("lots", []), "key")
	players = _by(data.get("players", []), "id")
	selected_lot = ""
	speed = int(data.get("clock", {}).get("speed", speed))
	if speed > 0:
		_resume_speed = speed
	_time.show_speed(speed)
	_call(world, "apply_snapshot", [data])
	_forward_common(data)
	_show_status(data)


func on_delta(data: Dictionary) -> void:
	var changed := _by(data.get("lots", []), "key")
	lots.merge(changed, true)
	players.merge(_by(data.get("players", []), "id"), true)
	_call(world, "apply_delta", [data])
	_forward_common(data)
	if changed.has(selected_lot):
		_call(lot_card, "update_lot", [lots[selected_lot]])
	_show_status(data)


func on_lot_clicked(key: String) -> void:
	if not lots.has(key):
		selected_lot = ""
		_call(lot_card, "hide_card", [])
		_call(world, "clear_highlight", [])
		return
	selected_lot = key
	_call(lot_card, "show_lot", [lots[key], players.values(), blocks.get(key, {}), auction])
	_call(world, "highlight_lot", [key])


func set_speed(value: int) -> void:
	speed = clampi(value, 0, 4)
	if speed > 0:
		_resume_speed = speed
	link.set_speed(speed)
	_time.show_speed(speed)


func toggle_pause() -> void:
	set_speed(_resume_speed if speed == 0 else 0)


## Opens the in-game menu and pauses the sim, remembering the speed to restore.
func open_menu() -> void:
	_menu_speed = speed
	set_speed(0)
	_menu.open()


func close_menu() -> void:
	_menu.close()
	set_speed(_menu_speed)


func quit_to_main_menu() -> void:
	_leave()
	exit_handler.call(true)


func quit_game() -> void:
	_leave()
	exit_handler.call(false)


func _unhandled_input(event: InputEvent) -> void:
	if event.is_action_pressed("menu"):
		if _menu.is_open():
			close_menu()
		else:
			open_menu()
	elif _menu.is_open():
		return
	elif event.is_action_pressed("pause_toggle"):
		toggle_pause()
	else:
		for n in range(1, 5):
			if event.is_action_pressed("speed_%d" % n):
				set_speed(n)
				break
		return
	get_viewport().set_input_as_handled()


func _notification(what: int) -> void:
	if what == NOTIFICATION_WM_CLOSE_REQUEST:
		_leave()


func _leave() -> void:
	if not _left and link != null:
		_left = true
		link.leave()


func _default_exit(to_menu: bool) -> void:
	if to_menu:
		get_tree().change_scene_to_file(MAIN_SCENE)
	else:
		get_tree().quit()


func _make_link() -> Node:
	var path := link_script_path()
	if path == "":
		return ClassDB.instantiate("SimLink") as Node
	return (load(path) as GDScript).new() as Node


static func _optional(path: String) -> Node:
	if not ResourceLoader.exists(path):
		return null
	return (load(path) as PackedScene).instantiate()


func _attach() -> void:
	link.name = "SimLink"
	_adopt(link, self)
	_adopt(world, self)
	if world != null:
		move_child(world, 0)
	for ui: Node in [hud, bid_feed, lot_card]:
		_adopt(ui, %UiRoot)
	if world == null:
		_status.text = "No world scene in this build; waiting for data…"


static func _adopt(child: Node, parent: Node) -> void:
	if child != null and child.get_parent() == null:
		parent.add_child(child)


func _connect_signals() -> void:
	link.snapshot_received.connect(on_snapshot)
	link.delta_received.connect(on_delta)
	link.game_over.connect(func(d: Dictionary) -> void: _status.text = "Game over: %s" % d.get("winner", "?"))
	link.link_error.connect(func(m: String) -> void: _status.text = "Link error: %s" % m)
	link.disconnected.connect(func() -> void: _status.text = "Disconnected")
	if world != null and world.has_signal("lot_clicked"):
		world.lot_clicked.connect(on_lot_clicked)
	_time.speed_requested.connect(set_speed)
	_time.pause_toggle_requested.connect(toggle_pause)
	_menu.continue_requested.connect(close_menu)
	_menu.quit_to_menu_requested.connect(quit_to_main_menu)
	_menu.quit_requested.connect(quit_game)


func _forward_common(data: Dictionary) -> void:
	if data.has("clock"):
		_call(hud, "update_clock", [data["clock"]])
	if data.has("players") and not data["players"].is_empty():
		_call(hud, "update_players", [players.values()])
	if not data.get("bids", []).is_empty():
		_call(bid_feed, "add_bids", [data["bids"], players.values(), blocks])
	if data.has("auction"):
		auction = data["auction"]
		_call(lot_card, "update_auction", [auction])
		_call(bid_feed, "update_auction", [auction, players.values(), blocks])


func _show_status(data: Dictionary) -> void:
	if world != null:
		_status.text = ""
		return
	var clock: Dictionary = data.get("clock", {})
	_status.text = "No world scene in this build. Tick %d, %d day %d %s, %d lots, %d walkers" % [
		int(data.get("tick", 0)), int(clock.get("year", 0)), int(clock.get("day", 0)),
		clock.get("time", "?"), lots.size(), floori(data.get("walkers", []).size() / 2.0)]


static func _call(target: Node, method: String, args: Array) -> void:
	if target != null and target.has_method(method):
		target.callv(method, args)


static func _by(items: Array, field: String) -> Dictionary:
	var out := {}
	for item: Variant in items:
		if item is Dictionary and (item as Dictionary).has(field):
			out[item[field]] = item
	return out


func _config_dir() -> String:
	return ProjectSettings.globalize_path("res://").path_join("../config").simplify_path()
