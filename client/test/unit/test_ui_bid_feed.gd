extends GutTest
## The bid feed (spec §10d #6): newest first, capped, readable lines, and a live-auction strip.

const BidFeedScene := preload("res://scenes/ui/bid_feed.tscn")

const PLAYERS := [
	{"id": "p0", "name": "The Grocer", "cash": 0, "net_worth": 0, "bankrupt": false},
	{"id": "p1", "name": "The Builder", "cash": 0, "net_worth": 0, "bankrupt": false},
]
const BLOCKS := {
	"mayfair": {"key": "mayfair", "name": "Mayfair", "district": "West End"},
	"old_kent_road": {"key": "old_kent_road", "name": "Old Kent Road", "district": "South"},
}

var feed: PanelContainer


func _bid(kind: String, player: Variant, amount: int, block := "mayfair", tick := 0) -> Dictionary:
	return {"tick": tick, "block": block, "player": player, "amount": amount, "kind": kind}


func before_each() -> void:
	feed = BidFeedScene.instantiate()
	add_child_autofree(feed)


func test_each_kind_reads_as_a_sentence() -> void:
	feed.add_bids([
		_bid("open", "p0", 100_000),
		_bid("raise", "p1", 120_000),
		_bid("pass", "p0", 0),
		_bid("won", "p1", 120_000),
		_bid("unsold", null, 0, "old_kent_road"),
	], PLAYERS, BLOCKS)
	assert_eq(feed.entry_texts(), PackedStringArray([
		"Old Kent Road went unsold",
		"Mayfair sold to The Builder for £1,200",
		"The Grocer passed on Mayfair",
		"The Builder raised Mayfair to £1,200",
		"The Grocer opened Mayfair at £1,000",
	]))


func test_newest_is_on_top_across_calls() -> void:
	feed.add_bids([_bid("open", "p0", 100_000, "mayfair", 1)], PLAYERS, BLOCKS)
	feed.add_bids([_bid("raise", "p1", 110_000, "mayfair", 2)], PLAYERS, BLOCKS)
	assert_eq(feed.entry_texts()[0], "The Builder raised Mayfair to £1,100")
	assert_eq(feed.entry_texts()[1], "The Grocer opened Mayfair at £1,000")


func test_unknown_block_and_player_fall_back_to_ids() -> void:
	feed.add_bids([_bid("raise", "p9", 5_000, "soho")], PLAYERS, BLOCKS)
	assert_eq(feed.entry_texts()[0], "p9 raised soho to £50")


func test_unknown_kind_still_shows_something() -> void:
	feed.add_bids([_bid("withdraw", "p0", 0)], PLAYERS, BLOCKS)
	assert_eq(feed.entry_texts()[0], "The Grocer: withdraw on Mayfair")


func test_blocks_may_map_keys_to_plain_names() -> void:
	feed.add_bids([_bid("won", "p0", 5_000)], PLAYERS, {"mayfair": "Mayfair"})
	assert_eq(feed.entry_texts()[0], "Mayfair sold to The Grocer for £50")


func test_feed_is_capped_keeping_the_newest() -> void:
	feed.max_entries = 5
	var bids: Array = []
	for i in 12:
		bids.append(_bid("raise", "p0", (i + 1) * 100, "mayfair", i))
	feed.add_bids(bids, PLAYERS, BLOCKS)
	var texts: PackedStringArray = feed.entry_texts()
	assert_eq(texts.size(), 5)
	assert_eq(texts[0], "The Grocer raised Mayfair to £12")
	assert_eq(texts[4], "The Grocer raised Mayfair to £8")


func test_cap_holds_over_many_calls() -> void:
	feed.max_entries = 3
	for i in 10:
		feed.add_bids([_bid("raise", "p0", (i + 1) * 100)], PLAYERS, BLOCKS)
	assert_eq(feed.entry_texts().size(), 3)
	assert_eq(feed.entry_texts()[0], "The Grocer raised Mayfair to £10")


func test_results_are_styled_apart_from_bids() -> void:
	feed.add_bids([_bid("raise", "p0", 100), _bid("won", "p0", 100), _bid("pass", "p1", 0)],
		PLAYERS, BLOCKS)
	var entries: Array[Node] = feed.get_node("%Entries").get_children()
	assert_eq((entries[0] as Label).theme_type_variation, &"FeedMuted", "pass")
	assert_eq((entries[1] as Label).theme_type_variation, &"FeedResult", "won")
	assert_eq((entries[2] as Label).theme_type_variation, &"FeedEntry", "raise")


func test_clear_empties_the_feed() -> void:
	feed.add_bids([_bid("raise", "p0", 100)], PLAYERS, BLOCKS)
	feed.clear()
	assert_eq(feed.entry_texts().size(), 0)


func test_auction_strip_shows_the_live_auction() -> void:
	feed.update_auction({"block": "mayfair", "item": "land", "high_bid": 150_000,
		"high_bidder": "p1", "seconds_left": 8, "round": 3}, PLAYERS, BLOCKS)
	assert_true(feed.get_node("%AuctionStrip").visible)
	assert_eq((feed.get_node("%AuctionLabel") as Label).text,
		"LIVE: Mayfair (land), £1,500 by The Builder, 8s, round 3")


func test_auction_strip_without_bidder() -> void:
	feed.update_auction({"block": "mayfair", "item": "land", "high_bid": 0, "high_bidder": null,
		"seconds_left": 120, "round": 1}, PLAYERS, BLOCKS)
	assert_eq((feed.get_node("%AuctionLabel") as Label).text,
		"LIVE: Mayfair (land), no bids yet, 120s, round 1")


func test_auction_strip_hides_when_no_auction() -> void:
	feed.update_auction({"block": "mayfair", "item": "land", "high_bid": 1, "high_bidder": null,
		"seconds_left": 1, "round": 1}, PLAYERS, BLOCKS)
	assert_true(feed.get_node("%AuctionStrip").visible)
	feed.update_auction(null, PLAYERS, BLOCKS)
	assert_false(feed.get_node("%AuctionStrip").visible)
