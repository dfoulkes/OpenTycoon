extends Control
## Preview of the in-game UI with sample data: the HUD, a lot card and the bid feed, with a
## pretend auction ticking down. Run this scene (F6) to see the panels together; no server.

const UiFixtures := preload("res://scripts/ui/ui_fixtures.gd")

const RAISE_PENCE := 50_000

var _players: Array = UiFixtures.players()
var _blocks: Dictionary = UiFixtures.blocks()
var _auction: Dictionary = UiFixtures.auction()
var _clock: Dictionary = UiFixtures.clock()
var _tick := 100


func _ready() -> void:
	%Hud.update_clock(_clock)
	%Hud.update_players(_players)
	%LotCard.show_lot(UiFixtures.lots()[0], _players, _blocks["mayfair"], _auction)
	%BidFeed.add_bids(UiFixtures.bids(), _players, _blocks)
	%BidFeed.update_auction(_auction, _players, _blocks)
	%Tick.timeout.connect(_on_tick)


## One pretend second: the countdown runs, and every few seconds someone raises.
func _on_tick() -> void:
	_tick += 1
	_auction["seconds_left"] = maxi(0, int(_auction["seconds_left"]) - 1)
	if int(_auction["seconds_left"]) == 0:
		var bidder := "p%d" % (_tick % 3)
		_auction["high_bid"] = int(_auction["high_bid"]) + RAISE_PENCE
		_auction["high_bidder"] = bidder
		_auction["seconds_left"] = 10
		%BidFeed.add_bids([{"tick": _tick, "block": "mayfair", "player": bidder,
			"amount": _auction["high_bid"], "kind": "raise"}], _players, _blocks)
	%LotCard.update_auction(_auction)
	%BidFeed.update_auction(_auction, _players, _blocks)
