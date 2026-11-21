extends RefCounted
## Sample data in the inspector protocol's shapes (docs/inspector-protocol.md), so each UI scene
## shows something when run on its own (F6) and the demo scene has a game to show.
## Invented names only.


static func players() -> Array:
	return [
		{"id": "p0", "name": "The Grocer", "cash": 700_000, "net_worth": 1_250_000, "bankrupt": false},
		{"id": "p1", "name": "The Builder", "cash": -180_000, "net_worth": 2_310_000, "bankrupt": false},
		{"id": "p2", "name": "The Hotelier", "cash": 45_050, "net_worth": 960_000, "bankrupt": false},
		{"id": "p3", "name": "The Banker", "cash": -1_200_000, "net_worth": -300_000, "bankrupt": true},
	]


static func blocks() -> Dictionary:
	return {
		"mayfair": {"key": "mayfair", "name": "Mayfair", "district": "West End", "set": "west_end"},
		"old_kent_road": {"key": "old_kent_road", "name": "Old Kent Road",
			"district": "Southwark", "set": "south"},
		"soho": {"key": "soho", "name": "Soho", "district": "West End", "set": "west_end"},
	}


static func lots() -> Array:
	return [
		{"key": "mayfair", "owner": "p1", "land_value": 1_850_000,
			"lease": {"years_left": 18},
			"building": {"kind": "shop", "name": "Arcade Buildings", "floors": 3, "flats": 4},
			"business": {"name": "Fine Teas", "kind": "grocer", "city_run": false},
			"takings_per_day": 62_000, "costs_per_day": 41_500, "profit_per_day": 20_500,
			"rent_per_day": 3_250, "open_for_bids": false, "last_sale_price": 1_500_000},
		{"key": "old_kent_road", "owner": "city", "land_value": 120_000, "lease": null,
			"building": {"kind": "market", "name": "", "floors": 1, "flats": 0},
			"business": {"name": "Riverside Market Hall", "kind": "market", "city_run": true},
			"takings_per_day": 9_000, "costs_per_day": 11_250, "profit_per_day": -2_250,
			"rent_per_day": 0, "open_for_bids": true, "last_sale_price": null},
		{"key": "soho", "owner": "unowned", "land_value": 0, "lease": null, "building": null,
			"business": null, "takings_per_day": 0, "costs_per_day": 0, "profit_per_day": 0,
			"rent_per_day": 0, "open_for_bids": true, "last_sale_price": null},
	]


static func auction() -> Dictionary:
	return {"block": "mayfair", "item": "land", "high_bid": 1_950_000, "high_bidder": "p2",
		"seconds_left": 7, "round": 3}


static func bids() -> Array:
	return [
		{"tick": 10, "block": "old_kent_road", "player": null, "amount": 0, "kind": "unsold"},
		{"tick": 40, "block": "mayfair", "player": "p0", "amount": 1_800_000, "kind": "open"},
		{"tick": 41, "block": "mayfair", "player": "p1", "amount": 1_850_000, "kind": "raise"},
		{"tick": 42, "block": "mayfair", "player": "p3", "amount": 0, "kind": "pass"},
		{"tick": 43, "block": "mayfair", "player": "p2", "amount": 1_950_000, "kind": "raise"},
	]


static func clock() -> Dictionary:
	return {"year": 1945, "day": 3, "time": "14:20", "speed": 2}
