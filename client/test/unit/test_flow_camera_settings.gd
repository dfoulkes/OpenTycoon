extends GutTest
## Camera settings (Settings autoload) reach the world's camera rig, at start and on change.

const GameScene := preload("res://scenes/game/game.tscn")
const SpyLink := preload("res://test/fixtures/spy_link.gd")
const World := preload("res://scenes/world/world.tscn")

var game: Node
var world: Node
var _saved := {}


func before_each() -> void:
	for key: String in ["pan_speed", "edge_pan", "invert_rotate"]:
		_saved[key] = Settings.get_value(key)
	Settings.path = "user://test_camera_settings.cfg"
	game = GameScene.instantiate()
	world = World.instantiate()
	game.exit_handler = func(_to_menu: bool) -> void: pass
	game.wire(SpyLink.new(), world, null, null, null)
	add_child_autofree(game)


func after_each() -> void:
	for key: String in _saved:
		Settings.set_value(key, _saved[key])
	DirAccess.remove_absolute(ProjectSettings.globalize_path(Settings.path))
	Settings.path = "user://settings.cfg"


func test_rig_takes_the_settings_at_start() -> void:
	var rig: Node = world.camera_rig()
	assert_eq(rig.pan_speed, float(Settings.get_value("pan_speed")))
	assert_eq(rig.invert_rotate, bool(Settings.get_value("invert_rotate")))


func test_rig_follows_a_changed_setting() -> void:
	Settings.set_value("pan_speed", 77.0)
	Settings.set_value("invert_rotate", true)
	Settings.set_value("edge_pan", false)
	var rig: Node = world.camera_rig()
	assert_eq(rig.pan_speed, 77.0)
	assert_true(rig.invert_rotate)
	assert_false(rig.edge_pan_enabled)
