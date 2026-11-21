extends PanelContainer
## The lot card: a property card for one lot (spec §10d #3, R88; look per R89). Fed dictionaries
## in the inspector protocol's shapes (docs/inspector-protocol.md); never touches the network.
## Run on its own (F6) it shows sample data.

signal closed

const Money := preload("res://scripts/ui/money.gd")
const UiText := preload("res://scripts/ui/ui_text.gd")
const UiFixtures := preload("res://scripts/ui/ui_fixtures.gd")

## Header band colours, picked per block `set` so a set's cards share a colour.
const SET_COLOURS: Array[Color] = [
	Color(0.18, 0.42, 0.247), Color(0.153, 0.306, 0.522), Color(0.545, 0.2, 0.18),
	Color(0.42, 0.267, 0.533), Color(0.749, 0.42, 0.122), Color(0.122, 0.478, 0.49),
	Color(0.416, 0.373, 0.169), Color(0.6, 0.208, 0.388),
]

var _lot: Dictionary = {}
var _players: Array = []
var _block: Dictionary = {}
var _auction: Dictionary = {}


func _ready() -> void:
	%CloseButton.pressed.connect(hide_card)
	if get_tree().current_scene == self:
		show_lot(UiFixtures.lots()[0], UiFixtures.players(), UiFixtures.blocks()["mayfair"],
			UiFixtures.auction())


## Shows `lot` (a protocol **lot**), naming owners from `players` and the lot from
## `board_block` (its snapshot block). Pass the live `auction` to show it if it is on this lot.
func show_lot(lot: Dictionary, players: Array, board_block: Dictionary,
		auction: Variant = null) -> void:
	_lot = lot
	_players = players
	_block = board_block
	_auction = auction if auction is Dictionary else {}
	_render_header()
	_render_stats()
	_render_auction()
	show()


## Refreshes the card in place when a delta changes the shown lot; ignores other lots.
func update_lot(lot: Dictionary) -> void:
	if _lot.is_empty() or lot.get("key") != _lot.get("key"):
		return
	_lot = lot
	_render_stats()


## The live auction from each delta (or null); shown only when it is on this lot.
func update_auction(auction: Variant) -> void:
	_auction = auction if auction is Dictionary else {}
	_render_auction()


func hide_card() -> void:
	_lot = {}
	hide()
	closed.emit()


## The key of the lot on show, or "" when the card is closed.
func shown_key() -> String:
	return str(_lot.get("key", ""))


func header_colour() -> Color:
	var box := %Header.get_theme_stylebox("panel") as StyleBoxFlat
	return box.bg_color if box else Color.BLACK


func _render_header() -> void:
	%NameLabel.text = str(_block.get("name", _lot.get("key", "")))
	%DistrictLabel.text = str(_block.get("district", ""))
	var box := (%Header.get_theme_stylebox("panel") as StyleBoxFlat).duplicate() as StyleBoxFlat
	var set_name := str(_block.get("set", _block.get("district", "")))
	box.bg_color = SET_COLOURS[absi(set_name.hash()) % SET_COLOURS.size()]
	%Header.add_theme_stylebox_override("panel", box)


func _render_stats() -> void:
	%OwnerValue.text = UiText.owner_name(_lot.get("owner"), _players)
	%LandValueValue.text = _money("land_value")
	%LeaseValue.text = UiText.lease_text(_lot.get("lease"))
	%BuildingValue.text = UiText.building_text(_lot.get("building"))
	%BusinessValue.text = UiText.business_text(_lot.get("business"))
	%CityRunBadge.visible = UiText.is_city_run(_lot.get("business"))
	%TakingsValue.text = _money("takings_per_day")
	%CostsValue.text = _money("costs_per_day")
	var profit := int(_lot.get("profit_per_day", 0))
	%ProfitValue.text = Money.format_signed(profit)
	%ProfitValue.theme_type_variation = UiText.money_variation(profit)
	%RentValue.text = _money("rent_per_day")
	%OpenForBidsValue.text = "Yes" if _lot.get("open_for_bids", false) else "No"
	%LastSaleValue.text = UiText.sale_text(_lot.get("last_sale_price"))


func _render_auction() -> void:
	var here: bool = not _lot.is_empty() and _auction.get("block") == _lot.get("key")
	%AuctionBox.visible = here
	if not here:
		return
	%AuctionHighValue.text = Money.format_pence(int(_auction.get("high_bid", 0)))
	var bidder: Variant = _auction.get("high_bidder")
	%AuctionBidderValue.text = "No bids yet" if bidder == null else UiText.player_name(bidder, _players)
	%AuctionTimeValue.text = "%ds (round %d)" % [int(_auction.get("seconds_left", 0)),
		int(_auction.get("round", 1))]


func _money(field: String) -> String:
	return Money.format_pence(int(_lot.get(field, 0)))
