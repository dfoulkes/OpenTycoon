extends GutTest
## The world scene built from protocol dictionaries (docs/inspector-protocol.md): board, lots,
## owner colours, city markers, the auction marker, highlight and picking. No network.

const WORLD_SCENE := "res://scenes/world/world.tscn"
const FIXTURE := "res://test/fixtures/world_snapshot.json"
const Palette := preload("res://scripts/world/palette.gd")

var world: Node3D


func before_each() -> void:
	var scene: PackedScene = load(WORLD_SCENE)
	world = scene.instantiate()
	add_child_autofree(world)


static func snapshot() -> Dictionary:
	return JSON.parse_string(FileAccess.get_file_as_string(FIXTURE))


func test_snapshot_builds_one_block_per_board_block() -> void:
	world.apply_snapshot(snapshot())
	assert_eq(world.block_count(), 9)


func test_blocks_sit_at_their_board_positions() -> void:
	world.apply_snapshot(snapshot())
	# Holborn: x 44, z 44, 36 m square, so its centre is (62, 62).
	var holborn: Node3D = world.block_node("holborn")
	assert_not_null(holborn)
	assert_almost_eq(holborn.position.x, 62.0, 0.001)
	assert_almost_eq(holborn.position.z, 62.0, 0.001)
	var mayfair: Node3D = world.block_node("mayfair")
	assert_almost_eq(mayfair.position.x, 102.0, 0.001)
	assert_almost_eq(mayfair.position.z, 102.0, 0.001)


func test_blocks_in_the_same_district_share_a_colour() -> void:
	world.apply_snapshot(snapshot())
	assert_eq(world.block_color("soho"), world.block_color("holborn"))
	assert_ne(world.block_color("soho"), world.block_color("mayfair"))


func test_snapshot_draws_streets_river_and_bridges() -> void:
	world.apply_snapshot(snapshot())
	assert_eq(world.feature_count("streets"), 7)
	assert_eq(world.feature_count("river"), 1)
	assert_eq(world.feature_count("bridges"), 1)


func test_a_second_snapshot_replaces_the_board() -> void:
	world.apply_snapshot(snapshot())
	world.apply_snapshot(snapshot())
	assert_eq(world.block_count(), 9)
	assert_eq(world.feature_count("streets"), 7)


func test_owned_lots_take_their_owner_colour() -> void:
	world.apply_snapshot(snapshot())
	assert_eq(world.lot_view("mayfair").owner_color(), Palette.owner_color("p1"))
	assert_eq(world.lot_view("holborn").owner_color(), Palette.owner_color("p0"))
	assert_ne(Palette.owner_color("p0"), Palette.owner_color("p1"))


func test_a_delta_changes_a_lots_owner_colour() -> void:
	world.apply_snapshot(snapshot())
	var lot: Dictionary = snapshot()["lots"][8].duplicate(true)
	lot["owner"] = "p0"
	world.apply_delta({ "lots": [lot], "auction": null, "walkers": [] })
	assert_eq(world.lot_view("mayfair").owner_color(), Palette.owner_color("p0"))
	assert_eq(world.lot_view("holborn").owner_color(), Palette.owner_color("p0"), "untouched")


func test_unowned_land_is_plain() -> void:
	world.apply_snapshot(snapshot())
	var soho = world.lot_view("soho")
	assert_false(soho.has_owner_pad())
	assert_false(soho.has_building())
	assert_false(soho.has_city_marker())


func test_building_height_comes_from_floors() -> void:
	world.apply_snapshot(snapshot())
	var holborn = world.lot_view("holborn")
	var mayfair = world.lot_view("mayfair")
	assert_true(holborn.has_building())
	assert_almost_eq(holborn.building_height(), 5.0 * Palette.FLOOR_HEIGHT_M, 0.01)
	assert_almost_eq(mayfair.building_height(), 2.0 * Palette.FLOOR_HEIGHT_M, 0.01)


func test_a_delta_that_demolishes_removes_the_building() -> void:
	world.apply_snapshot(snapshot())
	var lot: Dictionary = snapshot()["lots"][4].duplicate(true)
	lot["building"] = null
	world.apply_delta({ "lots": [lot] })
	assert_false(world.lot_view("holborn").has_building())


func test_city_lots_get_the_civic_colour_and_marker() -> void:
	world.apply_snapshot(snapshot())
	var whitechapel = world.lot_view("whitechapel")
	assert_true(whitechapel.has_city_marker())
	assert_eq(whitechapel.owner_color(), Palette.CIVIC)
	assert_false(world.lot_view("mayfair").has_city_marker())
	for id in ["p0", "p1", "p2", "p3", "p4", "p5"]:
		assert_ne(Palette.owner_color(id), Palette.CIVIC, "civic colour is not a player's")


func test_a_lot_sold_to_the_city_gains_the_marker_and_loses_it_when_sold_on() -> void:
	world.apply_snapshot(snapshot())
	var lot: Dictionary = snapshot()["lots"][3].duplicate(true)
	lot["owner"] = "city"
	world.apply_delta({ "lots": [lot] })
	assert_true(world.lot_view("soho").has_city_marker())
	lot["owner"] = "p1"
	world.apply_delta({ "lots": [lot] })
	assert_false(world.lot_view("soho").has_city_marker())


func test_snapshot_walkers_match_the_flat_array() -> void:
	world.apply_snapshot(snapshot())
	assert_eq(world.walker_count(), 3)


func test_delta_walkers_match_the_flat_array() -> void:
	world.apply_snapshot(snapshot())
	world.apply_delta({ "walkers": [1.0, 1.0, 2.0, 2.0, 3.0, 3.0, 4.0, 4.0, 5.0, 5.0] })
	assert_eq(world.walker_count(), 5)
	world.apply_delta({ "walkers": [] })
	assert_eq(world.walker_count(), 0)


func test_auction_marker_appears_over_its_lot_with_the_high_bid() -> void:
	world.apply_snapshot(snapshot())
	assert_false(world.auction_marker_visible())
	var auction := { "block": "soho", "item": "land", "high_bid": 123456700,
		"high_bidder": "p1", "seconds_left": 12, "round": 1 }
	world.apply_delta({ "auction": auction })
	assert_true(world.auction_marker_visible())
	assert_string_contains(world.auction_label(), "£1,234,567")
	var at: Vector3 = world.auction_marker_position()
	assert_almost_eq(at.x, 22.0, 0.001)
	assert_almost_eq(at.z, 62.0, 0.001)


func test_auction_marker_disappears_when_the_auction_ends() -> void:
	world.apply_snapshot(snapshot())
	world.apply_delta({ "auction": { "block": "soho", "item": "land", "high_bid": 0,
		"high_bidder": null, "seconds_left": 12, "round": 1 } })
	world.apply_delta({ "auction": null })
	assert_false(world.auction_marker_visible())


func test_a_delta_without_an_auction_field_keeps_the_marker() -> void:
	world.apply_snapshot(snapshot())
	world.apply_delta({ "auction": { "block": "soho", "item": "land", "high_bid": 100,
		"high_bidder": null, "seconds_left": 12, "round": 1 } })
	world.apply_delta({ "lots": [] })
	assert_true(world.auction_marker_visible())


func test_highlight_follows_the_lot_and_clears() -> void:
	world.apply_snapshot(snapshot())
	world.highlight_lot("mayfair")
	assert_eq(world.highlighted_lot(), "mayfair")
	assert_true(world.highlight_visible())
	world.highlight_lot("no_such_lot")
	assert_eq(world.highlighted_lot(), "", "unknown keys clear the highlight")
	world.highlight_lot("soho")
	world.clear_highlight()
	assert_eq(world.highlighted_lot(), "")
	assert_false(world.highlight_visible())


func test_a_ray_onto_a_lot_picks_its_key() -> void:
	world.apply_snapshot(snapshot())
	await wait_physics_frames(2)
	assert_eq(world.lot_key_at(Vector3(62, 100, 62), Vector3(62, -10, 62)), "holborn")
	assert_eq(world.lot_key_at(Vector3(42, 100, 42), Vector3(42, -10, 42)), "", "a street")


func test_selecting_a_lot_emits_lot_clicked() -> void:
	world.apply_snapshot(snapshot())
	watch_signals(world)
	await wait_physics_frames(2)
	world.select_at(Vector3(102, 100, 102), Vector3(102, -10, 102))
	assert_signal_emitted_with_parameters(world, "lot_clicked", ["mayfair"])
