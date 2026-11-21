extends GutTest
## The Settings autoload: defaults, save/load round trip through a ConfigFile, the `changed` signal.

const SettingsScript := preload("res://scripts/settings.gd")
const PATH := "user://test_settings.cfg"


func _fresh() -> Node:
	var s: Node = SettingsScript.new()
	s.path = PATH
	add_child_autofree(s)
	return s


func before_each() -> void:
	DirAccess.remove_absolute(PATH)


func after_all() -> void:
	DirAccess.remove_absolute(PATH)


func test_autoload_is_registered() -> void:
	assert_not_null(get_node_or_null("/root/Settings"), "Settings autoload in project.godot")


func test_missing_file_gives_defaults() -> void:
	var s := _fresh()
	s.load_settings()
	assert_eq(s.get_value("vsync"), true)
	assert_eq(s.get_value("fullscreen"), false)
	assert_eq(s.get_value("edge_pan"), true)
	assert_eq(s.get_value("invert_rotate"), false)


func test_save_load_round_trip() -> void:
	var a := _fresh()
	a.set_value("msaa", 2)
	a.set_value("vsync", false)
	a.set_value("pan_speed", 72.5)
	a.set_value("edge_pan", false)
	a.set_value("invert_rotate", true)
	assert_true(FileAccess.file_exists(PATH), "set_value saves")
	var b := _fresh()
	b.load_settings()
	assert_eq(b.get_value("msaa"), 2)
	assert_eq(b.get_value("vsync"), false)
	assert_almost_eq(float(b.get_value("pan_speed")), 72.5, 0.001)
	assert_eq(b.get_value("edge_pan"), false)
	assert_eq(b.get_value("invert_rotate"), true)


func test_set_value_emits_changed_with_key() -> void:
	var s := _fresh()
	watch_signals(s)
	s.set_value("invert_rotate", true)
	assert_signal_emitted_with_parameters(s, "changed", ["invert_rotate"])


func test_unknown_key_is_rejected() -> void:
	var s := _fresh()
	watch_signals(s)
	s.set_value("no_such_setting", 1)
	assert_signal_not_emitted(s, "changed")
	assert_null(s.get_value("no_such_setting"))
