extends GutTest
## The top HUD (spec §10d #7): the clock, the speed (paused included) and each player's standing.

const HudScene := preload("res://scenes/ui/hud.tscn")

var hud: PanelContainer


func _player(id: String, name: String, cash: int, worth: int, bankrupt := false) -> Dictionary:
	return {"id": id, "name": name, "cash": cash, "net_worth": worth, "bankrupt": bankrupt}


func _text(path: String) -> String:
	return (hud.get_node(path) as Label).text


func before_each() -> void:
	hud = HudScene.instantiate()
	add_child_autofree(hud)


func test_clock_shows_year_day_and_time() -> void:
	hud.update_clock({"year": 1930, "day": 0, "time": "06:00", "speed": 1})
	assert_eq(_text("%ClockLabel"), "1930  ·  Day 1  ·  06:00")
	assert_eq(_text("%SpeedLabel"), "Speed 1")


func test_speed_steps() -> void:
	for speed: int in [1, 2, 3, 4]:
		hud.update_clock({"year": 1935, "day": 1, "time": "12:30", "speed": speed})
		assert_eq(_text("%SpeedLabel"), "Speed %d" % speed)
		assert_eq((hud.get_node("%SpeedLabel") as Label).theme_type_variation, &"HudLabel")


func test_paused_state() -> void:
	hud.update_clock({"year": 1940, "day": 2, "time": "17:10", "speed": 0})
	assert_eq(_text("%SpeedLabel"), "PAUSED")
	assert_eq((hud.get_node("%SpeedLabel") as Label).theme_type_variation, &"HudPaused")
	assert_true(hud.is_paused())
	hud.update_clock({"year": 1940, "day": 2, "time": "17:10", "speed": 2})
	assert_false(hud.is_paused())
	assert_eq(_text("%SpeedLabel"), "Speed 2")


func test_players_show_name_cash_and_net_worth() -> void:
	hud.update_players([
		_player("p0", "The Grocer", 700_000, 700_000),
		_player("p1", "The Builder", -150_000, 2_500_000),
	])
	assert_eq(_text("%Players/p0/Name"), "The Grocer")
	assert_eq(_text("%Players/p0/Cash"), "Cash £7,000")
	assert_eq(_text("%Players/p0/Worth"), "Worth £7,000")
	assert_eq(_text("%Players/p1/Cash"), "Cash −£1,500")
	assert_eq(_text("%Players/p1/Worth"), "Worth £25,000")
	assert_eq(hud.get_node("%Players").get_child_count(), 2)


func test_partial_update_replaces_by_id_and_keeps_order() -> void:
	hud.update_players([_player("p0", "The Grocer", 100, 100), _player("p1", "The Builder", 200, 200)])
	hud.update_players([_player("p1", "The Builder", 900, 900)])
	var rows: Array[Node] = hud.get_node("%Players").get_children()
	assert_eq(rows.size(), 2)
	assert_eq(rows[0].name, &"p0")
	assert_eq(_text("%Players/p1/Cash"), "Cash £9")
	assert_eq(_text("%Players/p0/Cash"), "Cash £1")


func test_bankrupt_player_is_marked() -> void:
	hud.update_players([_player("p2", "The Banker", -2_000_000, -500_000, true)])
	assert_eq(_text("%Players/p2/Name"), "The Banker (bankrupt)")
	assert_eq((hud.get_node("%Players/p2/Name") as Label).theme_type_variation, &"HudBankrupt")
	assert_lt((hud.get_node("%Players/p2") as Control).modulate.a, 1.0)


func test_hud_spans_the_top_of_the_screen() -> void:
	assert_eq(hud.anchor_left, 0.0)
	assert_eq(hud.anchor_right, 1.0)
	assert_eq(hud.anchor_top, 0.0)
	assert_eq(hud.anchor_bottom, 0.0)
