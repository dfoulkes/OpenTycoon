extends PanelContainer
## The bid feed (spec §10d #6): recent bids and results, newest on top, capped at
## `max_entries`, plus a strip for the live auction. Fed protocol **bid** and **auction**
## dictionaries (docs/inspector-protocol.md). Run on its own (F6) it shows sample data.

const UiText := preload("res://scripts/ui/ui_text.gd")
const UiFixtures := preload("res://scripts/ui/ui_fixtures.gd")

## The oldest lines drop off past this many.
@export_range(1, 500) var max_entries: int = 40


func _ready() -> void:
	if get_tree().current_scene == self:
		add_bids(UiFixtures.bids(), UiFixtures.players(), UiFixtures.blocks())
		update_auction(UiFixtures.auction(), UiFixtures.players(), UiFixtures.blocks())


## Adds new bid events (oldest first, as the protocol sends them) to the top of the feed.
## `blocks` maps block key -> snapshot block (or -> name).
func add_bids(bids: Array, players: Array, blocks: Dictionary) -> void:
	for bid: Variant in bids:
		if not bid is Dictionary:
			continue
		var line := Label.new()
		line.text = UiText.bid_line(bid, players, blocks)
		line.theme_type_variation = UiText.bid_variation(bid.get("kind"))
		line.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		%Entries.add_child(line)
		%Entries.move_child(line, 0)
	_trim()


## Shows the live auction in the strip, or hides the strip when `auction` is null.
func update_auction(auction: Variant, players: Array, blocks: Dictionary) -> void:
	var text := UiText.auction_line(auction, players, blocks)
	%AuctionStrip.visible = text != ""
	%AuctionLabel.text = text


func clear() -> void:
	for line: Node in %Entries.get_children():
		line.free()


## The feed's lines, newest first.
func entry_texts() -> PackedStringArray:
	var texts := PackedStringArray()
	for line: Node in %Entries.get_children():
		texts.append((line as Label).text)
	return texts


func _trim() -> void:
	while %Entries.get_child_count() > max_entries:
		var oldest: Node = %Entries.get_child(%Entries.get_child_count() - 1)
		oldest.free()
