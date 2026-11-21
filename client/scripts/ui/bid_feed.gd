extends PanelContainer
## Bid feed (stub: red step).

@export var max_entries: int = 40


func add_bids(_bids: Array, _players: Array, _blocks: Dictionary) -> void:
	pass


func update_auction(_auction: Variant, _players: Array, _blocks: Dictionary) -> void:
	pass


func clear() -> void:
	pass


func entry_texts() -> PackedStringArray:
	return PackedStringArray()
