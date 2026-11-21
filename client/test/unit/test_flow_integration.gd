extends GutTest
## The game controller wired to the real HUD, lot card and bid feed (spy link and world): the
## shapes each side passes must agree (players as an Array, the live auction reaching the card).

const GameScene := preload("res://scenes/game/game.tscn")
const SpyLink := preload("res://test/fixtures/spy_link.gd")
const SpyWorld := preload("res://test/fixtures/spy_world.gd")
const Hud := preload("res://scenes/ui/hud.tscn")
const LotCard := preload("res://scenes/ui/lot_card.tscn")
const BidFeed := preload("res://scenes/ui/bid_feed.tscn")
const FIXTURE := "res://test/fixtures/snapshot_small.json"

var game: Node
var link: Node
var world: Node
var hud: Control
var card: Control
var feed: Control


func before_each() -> void:
	game = GameScene.instantiate()
	link = SpyLink.new()
	world = SpyWorld.new()
	hud = Hud.instantiate()
	card = LotCard.instantiate()
	feed = BidFeed.instantiate()
	game.exit_handler = func(_to_menu: bool) -> void: pass
	game.wire(link, world, hud, card, feed)
	add_child_autofree(game)
	link.snapshot_received.emit(JSON.parse_string(FileAccess.get_file_as_string(FIXTURE)))


func _texts(node: Node) -> String:
	var out := ""
	for child in node.find_children("*", "Label", true, false):
		out += (child as Label).text + "\n"
	return out


func test_hud_lists_the_players_by_name() -> void:
	assert_string_contains(_texts(hud), "The Grocer")
	assert_string_contains(_texts(hud), "The Landlord")


func test_clicking_a_player_lot_shows_its_owner_by_name() -> void:
	world.lot_clicked.emit("bermondsey_street")
	assert_eq(card.shown_key(), "bermondsey_street")
	assert_eq(card.get_node("%OwnerValue").text, "The Grocer")


func test_clicking_the_lot_under_auction_shows_the_live_bid() -> void:
	world.lot_clicked.emit("southwark_street")
	assert_true(card.get_node("%AuctionBox").visible, "auction box shown")
	assert_string_contains(card.get_node("%AuctionBidderValue").text, "The Landlord")


func test_bid_feed_names_players_not_ids() -> void:
	var lines := "\n".join(feed.entry_texts())
	assert_false(lines.is_empty(), "feed has entries")
	assert_false(lines.contains(" p0 ") or lines.begins_with("p0") or lines.begins_with("p1"),
		"players resolved to names: " + lines)


func test_money_from_json_floats_formats_as_pounds() -> void:
	world.lot_clicked.emit("bermondsey_street")
	var value: String = card.get_node("%LandValueValue").text
	assert_true(value.begins_with("£"), "land value formatted: " + value)
	assert_false(value.contains("."), "whole pounds, no float artefacts: " + value)
