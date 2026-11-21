extends GutTest
## End to end: the real game scene on the real server (SimLink, in-process on localhost), with
## the real world and UI scenes. Proves the client shows the real 9×9 game, not sample data.

const GameScene := preload("res://scenes/game/game.tscn")

var game: Node


func before_each() -> void:
	if not ClassDB.class_exists("SimLink"):
		return
	game = GameScene.instantiate()
	game.exit_handler = func(_to_menu: bool) -> void: pass
	game.game_seed = 42
	add_child_autofree(game)


func test_real_game_shows_the_full_board_and_a_lot_card() -> void:
	if not ClassDB.class_exists("SimLink"):
		pending("SimLink not built: cargo build -p client-ext")
		return
	assert_eq(game.link_mode, "real")
	assert_false(game.sample_banner_visible(), "no sample banner on the real game")
	await wait_until(func() -> bool: return not game.lots.is_empty(), 30, "snapshot")
	assert_eq(game.lots.size(), 81, "the 9×9 board's lots")
	assert_eq(game.world.block_count(), 81)
	assert_gt(game.players.size(), 1, "AI players seated")
	var key: String = game.lots.keys()[0]
	game.world.lot_clicked.emit(key)
	assert_eq(game.lot_card.shown_key(), key)
	game.set_speed(4)
	assert_true(await wait_for_signal(game.link.delta_received, 10, "delta"), "deltas flow at speed 4")
	game.quit_to_main_menu()
