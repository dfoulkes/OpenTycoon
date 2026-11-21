extends PanelContainer
## The top HUD (spec §10d #7): game clock, speed (paused included) and every player's cash and
## net worth. Fed protocol **clock** and **player** dictionaries; a delta's `players` holds only
## the changed ones, so rows are replaced by id. Run on its own (F6) it shows sample data.

const Money := preload("res://scripts/ui/money.gd")
const UiFixtures := preload("res://scripts/ui/ui_fixtures.gd")

const BANKRUPT_ALPHA := 0.55

var _paused := false


func _ready() -> void:
	if get_tree().current_scene == self:
		update_clock(UiFixtures.clock())
		update_players(UiFixtures.players())


## `clock`: `{year, day, time, speed}`; speed 0 is paused. Days show 1-based.
func update_clock(clock: Dictionary) -> void:
	%ClockLabel.text = "%d  ·  Day %d  ·  %s" % [int(clock.get("year", 0)),
		int(clock.get("day", 0)) + 1, str(clock.get("time", ""))]
	var speed := int(clock.get("speed", 0))
	_paused = speed == 0
	%SpeedLabel.text = "PAUSED" if _paused else "Speed %d" % speed
	%SpeedLabel.theme_type_variation = &"HudPaused" if _paused else &"HudLabel"


func is_paused() -> bool:
	return _paused


## Adds or replaces each player's row by `id`; rows keep their first-seen order.
func update_players(players: Array) -> void:
	for player: Variant in players:
		if player is Dictionary:
			_render_player(_row_for(str(player.get("id", "?"))), player)


func _row_for(id: String) -> VBoxContainer:
	var row := %Players.get_node_or_null(NodePath(id)) as VBoxContainer
	if row:
		return row
	row = VBoxContainer.new()
	row.name = id
	row.add_theme_constant_override("separation", 0)
	for part: String in ["Name", "Cash", "Worth"]:
		var label := Label.new()
		label.name = part
		label.theme_type_variation = &"HudLabel"
		row.add_child(label)
	%Players.add_child(row)
	return row


func _render_player(row: VBoxContainer, player: Dictionary) -> void:
	var bankrupt := bool(player.get("bankrupt", false))
	var name_label := row.get_node("Name") as Label
	name_label.text = str(player.get("name", row.name)) + (" (bankrupt)" if bankrupt else "")
	name_label.theme_type_variation = &"HudBankrupt" if bankrupt else &"HudLabel"
	(row.get_node("Cash") as Label).text = "Cash " + Money.format_pence(int(player.get("cash", 0)))
	(row.get_node("Worth") as Label).text = "Worth " + Money.format_pence(
		int(player.get("net_worth", 0)))
	row.modulate.a = BANKRUPT_ALPHA if bankrupt else 1.0
