extends PanelContainer
## Lot card (stub: red step).

signal closed


func show_lot(_lot: Dictionary, _players: Array, _board_block: Dictionary,
		_auction: Variant = null) -> void:
	pass


func update_lot(_lot: Dictionary) -> void:
	pass


func update_auction(_auction: Variant) -> void:
	pass


func hide_card() -> void:
	pass


func shown_key() -> String:
	return "?"


func header_colour() -> Color:
	return Color.BLACK
