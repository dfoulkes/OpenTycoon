extends GutTest
## Never show made-up data silently: the fake link runs only when asked for (--sample-data or
## OPENTYCOON_SAMPLE_DATA=1) and then a SAMPLE DATA banner says so; with no real server built,
## the game says so instead of falling back.

const GameScene := preload("res://scenes/game/game.tscn")
const SpyLink := preload("res://test/fixtures/spy_link.gd")
const Controller := preload("res://scripts/game/game_controller.gd")


func _game(mode: String) -> Array:
	var game: Node = GameScene.instantiate()
	var link: Node = SpyLink.new()
	game.exit_handler = func(_to_menu: bool) -> void: pass
	game.link_mode = mode
	game.wire(link, null, null, null, null)
	add_child_autofree(game)
	return [game, link]


func test_real_server_wins_when_built_and_not_asked_for_samples() -> void:
	assert_eq(Controller.choose_link_mode(true, false), "real")


func test_no_server_and_no_request_is_missing_not_fake() -> void:
	assert_eq(Controller.choose_link_mode(false, false), "missing")


func test_asking_for_samples_gets_the_fake_either_way() -> void:
	assert_eq(Controller.choose_link_mode(true, true), "fake")
	assert_eq(Controller.choose_link_mode(false, true), "fake")


func test_sample_data_shows_a_banner() -> void:
	var g: Array = _game("fake")
	assert_true(g[0].sample_banner_visible(), "banner shown on sample data")


func test_real_data_has_no_banner() -> void:
	var g: Array = _game("real")
	assert_false(g[0].sample_banner_visible())


func test_missing_server_says_so_and_starts_nothing() -> void:
	var g: Array = _game("missing")
	assert_eq(g[1].called("start_single_player").size(), 0)
	assert_string_contains(g[0].get_node("%Status").text, "cargo build -p client-ext")
