extends Control
## Test spy standing in for the HUD, lot card and bid feed. Records every call in `calls`.

var calls: Array = []


func show_lot(lot: Dictionary, players: Dictionary, board_block: Dictionary) -> void:
	calls.append(["show_lot", [lot, players, board_block]])


func update_lot(lot: Dictionary) -> void:
	calls.append(["update_lot", [lot]])


func hide_card() -> void:
	calls.append(["hide_card", []])


func add_bids(bids: Array, players: Dictionary, blocks: Dictionary) -> void:
	calls.append(["add_bids", [bids, players, blocks]])


func update_clock(clock: Dictionary) -> void:
	calls.append(["update_clock", [clock]])


func update_players(players: Dictionary) -> void:
	calls.append(["update_players", [players]])


func called(method: String) -> Array:
	var out: Array = []
	for c: Array in calls:
		if c[0] == method:
			out.append(c[1])
	return out
