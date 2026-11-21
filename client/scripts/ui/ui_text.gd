extends RefCounted
## Pure text helpers shared by the inspector UI: names for players, owners and blocks, and the
## wording of lot fields and bid events. No nodes, so every rule here is cheap to test.

const Money := preload("res://scripts/ui/money.gd")


## A player's display name from the `players` array, or the id if it is not there.
static func player_name(id: Variant, players: Array) -> String:
	if id == null:
		return "Nobody"
	for player: Variant in players:
		if player is Dictionary and player.get("id") == id:
			return str(player.get("name", id))
	return str(id)


## A lot owner: a player's name, "City" or "Unowned".
static func owner_name(owner: Variant, players: Array) -> String:
	if owner == null or owner == "unowned":
		return "Unowned"
	if owner == "city":
		return "City"
	return player_name(owner, players)


## A block's display name. `blocks` maps key -> block dictionary or key -> plain name.
static func block_name(key: Variant, blocks: Dictionary) -> String:
	var entry: Variant = blocks.get(key)
	if entry is Dictionary:
		return str(entry.get("name", key))
	if entry is String:
		return entry
	return str(key)


## A value as text, with null as "".
static func text_or_empty(v: Variant) -> String:
	return "" if v == null else str(v)


## "corner_shop" -> "Corner Shop".
static func humanise(kind: Variant) -> String:
	return str(kind).capitalize()


## "1 floor", "3 flats".
static func count(n: int, singular: String, plural: String) -> String:
	return "%d %s" % [n, singular if n == 1 else plural]


static func lease_text(lease: Variant) -> String:
	if not lease is Dictionary:
		return "No lease"
	return count(int(lease.get("years_left", 0)), "year", "years") + " left"


static func building_text(building: Variant) -> String:
	if not building is Dictionary:
		return "Empty plot"
	var kind := humanise(building.get("kind", ""))
	var name := text_or_empty(building.get("name"))
	var head := ("%s (%s)" % [name, kind]) if name != "" else kind
	return "%s, %s, %s" % [head, count(int(building.get("floors", 0)), "floor", "floors"),
		count(int(building.get("flats", 0)), "flat", "flats")]


static func business_text(business: Variant) -> String:
	if not business is Dictionary:
		return "None"
	var kind := humanise(business.get("kind", ""))
	var name := text_or_empty(business.get("name"))
	return ("%s (%s)" % [name, kind]) if name != "" else kind


static func is_city_run(business: Variant) -> bool:
	return business is Dictionary and bool(business.get("city_run", false))


static func sale_text(price: Variant) -> String:
	return "Never sold" if price == null else Money.format_pence(int(price))


## The theme variation for a money figure: green gains, red losses, plain zero.
static func money_variation(p: int) -> StringName:
	if p > 0:
		return &"MoneyPositive"
	if p < 0:
		return &"MoneyNegative"
	return &""


## One feed line for a bid event (docs/inspector-protocol.md, **bid**).
static func bid_line(bid: Dictionary, players: Array, blocks: Dictionary) -> String:
	var who := player_name(bid.get("player"), players)
	var where := block_name(bid.get("block"), blocks)
	var amount := Money.format_pence(int(bid.get("amount", 0)))
	var kind := str(bid.get("kind", ""))
	match kind:
		"open":
			return "%s opened %s at %s" % [who, where, amount]
		"raise":
			return "%s raised %s to %s" % [who, where, amount]
		"pass":
			return "%s passed on %s" % [who, where]
		"won":
			return "%s sold to %s for %s" % [where, who, amount]
		"unsold":
			return "%s went unsold" % where
	return "%s: %s on %s" % [who, kind, where]


## Results stand out, passes fade, bids are plain.
static func bid_variation(kind: Variant) -> StringName:
	match kind:
		"won", "unsold":
			return &"FeedResult"
		"pass":
			return &"FeedMuted"
	return &"FeedEntry"


## The live-auction strip, or "" when there is no auction.
static func auction_line(auction: Variant, players: Array, blocks: Dictionary) -> String:
	if not auction is Dictionary:
		return ""
	var bidder: Variant = auction.get("high_bidder")
	var bid := "no bids yet" if bidder == null else "%s by %s" % [
		Money.format_pence(int(auction.get("high_bid", 0))), player_name(bidder, players)]
	return "LIVE: %s (%s), %s, %ds, round %d" % [block_name(auction.get("block"), blocks),
		str(auction.get("item", "land")), bid, int(auction.get("seconds_left", 0)),
		int(auction.get("round", 1))]
