extends GutTest
## The lot card (spec §10d #3, R88): every field renders, absent parts read as such,
## profit is coloured by sign, and live updates land in place.

const LotCardScene := preload("res://scenes/ui/lot_card.tscn")

const PLAYERS := [
	{"id": "p0", "name": "The Grocer", "cash": 700_000, "net_worth": 700_000, "bankrupt": false},
	{"id": "p1", "name": "The Builder", "cash": 250_000, "net_worth": 900_000, "bankrupt": false},
]
const BLOCK := {"key": "mayfair", "name": "Mayfair", "district": "West End", "set": "west_end"}

var card: PanelContainer


func _lot(overrides: Dictionary = {}) -> Dictionary:
	var lot := {
		"key": "mayfair", "owner": "p1", "land_value": 1_250_000,
		"lease": {"years_left": 12},
		"building": {"kind": "shop", "name": "Corner Stores", "floors": 2, "flats": 3},
		"business": {"name": "Daily Bread", "kind": "bakery", "city_run": false},
		"takings_per_day": 45_000, "costs_per_day": 30_000, "profit_per_day": 15_000,
		"rent_per_day": 2_550, "open_for_bids": true, "last_sale_price": 900_000,
	}
	lot.merge(overrides, true)
	return lot


func _text(unique: String) -> String:
	return (card.get_node("%" + unique) as Label).text


func before_each() -> void:
	card = LotCardScene.instantiate()
	add_child_autofree(card)


func test_every_field_renders() -> void:
	card.show_lot(_lot(), PLAYERS, BLOCK)
	assert_true(card.visible)
	assert_eq(_text("NameLabel"), "Mayfair")
	assert_eq(_text("DistrictLabel"), "West End")
	assert_eq(_text("OwnerValue"), "The Builder")
	assert_eq(_text("LandValueValue"), "£12,500")
	assert_eq(_text("LeaseValue"), "12 years left")
	assert_eq(_text("BuildingValue"), "Corner Stores (Shop), 2 floors, 3 flats")
	assert_eq(_text("BusinessValue"), "Daily Bread (Bakery)")
	assert_false(card.get_node("%CityRunBadge").visible)
	assert_eq(_text("TakingsValue"), "£450")
	assert_eq(_text("CostsValue"), "£300")
	assert_eq(_text("ProfitValue"), "+£150")
	assert_eq(_text("RentValue"), "£25.50")
	assert_eq(_text("OpenForBidsValue"), "Yes")
	assert_eq(_text("LastSaleValue"), "£9,000")
	assert_eq(card.shown_key(), "mayfair")


func test_city_and_unowned_owners() -> void:
	card.show_lot(_lot({"owner": "city"}), PLAYERS, BLOCK)
	assert_eq(_text("OwnerValue"), "City")
	card.show_lot(_lot({"owner": "unowned"}), PLAYERS, BLOCK)
	assert_eq(_text("OwnerValue"), "Unowned")


func test_unknown_player_falls_back_to_the_id() -> void:
	card.show_lot(_lot({"owner": "p7"}), PLAYERS, BLOCK)
	assert_eq(_text("OwnerValue"), "p7")


func test_missing_block_falls_back_to_the_lot_key() -> void:
	card.show_lot(_lot(), PLAYERS, {})
	assert_eq(_text("NameLabel"), "mayfair")
	assert_eq(_text("DistrictLabel"), "")


func test_null_lease_building_business_and_sale() -> void:
	card.show_lot(
		_lot({"lease": null, "building": null, "business": null, "last_sale_price": null,
			"open_for_bids": false}),
		PLAYERS, BLOCK)
	assert_eq(_text("LeaseValue"), "No lease")
	assert_eq(_text("BuildingValue"), "Empty plot")
	assert_eq(_text("BusinessValue"), "None")
	assert_false(card.get_node("%CityRunBadge").visible)
	assert_eq(_text("LastSaleValue"), "Never sold")
	assert_eq(_text("OpenForBidsValue"), "No")


func test_singular_units() -> void:
	card.show_lot(
		_lot({"lease": {"years_left": 1},
			"building": {"kind": "corner_shop", "name": "", "floors": 1, "flats": 1}}),
		PLAYERS, BLOCK)
	assert_eq(_text("LeaseValue"), "1 year left")
	assert_eq(_text("BuildingValue"), "Corner Shop, 1 floor, 1 flat")


func test_city_run_business_shows_the_badge() -> void:
	card.show_lot(
		_lot({"owner": "city",
			"business": {"name": "Town Market", "kind": "market", "city_run": true}}),
		PLAYERS, BLOCK)
	assert_true(card.get_node("%CityRunBadge").visible)
	assert_eq(_text("CityRunBadge"), "CITY-RUN")


func test_profit_is_green_when_positive() -> void:
	card.show_lot(_lot({"profit_per_day": 100}), PLAYERS, BLOCK)
	var label: Label = card.get_node("%ProfitValue")
	assert_eq(label.theme_type_variation, &"MoneyPositive")
	var colour := label.get_theme_color("font_color")
	assert_gt(colour.g, colour.r, "positive profit reads green")


func test_profit_is_red_when_negative() -> void:
	card.show_lot(_lot({"profit_per_day": -4_200}), PLAYERS, BLOCK)
	var label: Label = card.get_node("%ProfitValue")
	assert_eq(label.text, "−£42")
	assert_eq(label.theme_type_variation, &"MoneyNegative")
	var colour := label.get_theme_color("font_color")
	assert_gt(colour.r, colour.g, "a loss reads red")


func test_zero_profit_is_neutral() -> void:
	card.show_lot(_lot({"profit_per_day": 0}), PLAYERS, BLOCK)
	var label: Label = card.get_node("%ProfitValue")
	assert_eq(label.text, "£0")
	assert_eq(label.theme_type_variation, &"")


func test_auction_on_this_lot_shows_live_bid() -> void:
	var auction := {"block": "mayfair", "item": "land", "high_bid": 120_000, "high_bidder": "p0",
		"seconds_left": 7, "round": 2}
	card.show_lot(_lot(), PLAYERS, BLOCK, auction)
	assert_true(card.get_node("%AuctionBox").visible)
	assert_eq(_text("AuctionHighValue"), "£1,200")
	assert_eq(_text("AuctionBidderValue"), "The Grocer")
	assert_eq(_text("AuctionTimeValue"), "7s (round 2)")


func test_auction_elsewhere_is_hidden() -> void:
	var auction := {"block": "old_kent_road", "item": "land", "high_bid": 1, "high_bidder": null,
		"seconds_left": 3, "round": 1}
	card.show_lot(_lot(), PLAYERS, BLOCK, auction)
	assert_false(card.get_node("%AuctionBox").visible)


func test_update_auction_toggles_the_box() -> void:
	card.show_lot(_lot(), PLAYERS, BLOCK)
	assert_false(card.get_node("%AuctionBox").visible)
	card.update_auction({"block": "mayfair", "item": "land", "high_bid": 5_000,
		"high_bidder": null, "seconds_left": 90, "round": 1})
	assert_true(card.get_node("%AuctionBox").visible)
	assert_eq(_text("AuctionBidderValue"), "No bids yet")
	card.update_auction(null)
	assert_false(card.get_node("%AuctionBox").visible)


func test_update_lot_refreshes_the_shown_lot_in_place() -> void:
	card.show_lot(_lot(), PLAYERS, BLOCK)
	card.update_lot(_lot({"owner": "p0", "profit_per_day": -100, "land_value": 2_000_000}))
	assert_eq(_text("OwnerValue"), "The Grocer")
	assert_eq(_text("ProfitValue"), "−£1")
	assert_eq(_text("LandValueValue"), "£20,000")
	assert_eq(_text("NameLabel"), "Mayfair", "block details kept")


func test_update_lot_ignores_other_lots() -> void:
	card.show_lot(_lot(), PLAYERS, BLOCK)
	card.update_lot(_lot({"key": "old_kent_road", "owner": "city"}))
	assert_eq(_text("OwnerValue"), "The Builder")


func test_hide_card_hides_and_forgets_the_lot() -> void:
	card.show_lot(_lot(), PLAYERS, BLOCK)
	watch_signals(card)
	card.hide_card()
	assert_false(card.visible)
	assert_eq(card.shown_key(), "")
	assert_signal_emitted(card, "closed")
	card.update_lot(_lot({"owner": "city"}))
	assert_false(card.visible, "a delta does not reopen a closed card")


func test_close_button_hides_the_card() -> void:
	card.show_lot(_lot(), PLAYERS, BLOCK)
	(card.get_node("%CloseButton") as Button).pressed.emit()
	assert_false(card.visible)


func test_header_band_takes_the_set_colour() -> void:
	card.show_lot(_lot(), PLAYERS, BLOCK)
	var a: Color = card.header_colour()
	card.show_lot(_lot(), PLAYERS, {"key": "mayfair", "name": "Mayfair", "set": "west_end"})
	assert_eq(card.header_colour(), a, "same set, same colour")


func test_card_has_the_shared_theme() -> void:
	assert_not_null(card.theme)
	assert_eq(card.theme.resource_path, "res://theme/inspector_theme.tres")
