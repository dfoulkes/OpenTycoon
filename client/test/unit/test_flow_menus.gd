extends GutTest
## Front-end flow: the main menu's buttons switch panels and every Back returns.

const MainScene := preload("res://scenes/main.tscn")
const NewGameScene := preload("res://scenes/menus/new_game_menu.tscn")
const SettingsScene := preload("res://scenes/menus/settings_menu.tscn")
const SettingsScript := preload("res://scripts/settings.gd")


func _main() -> Control:
	var m: Control = MainScene.instantiate()
	add_child_autofree(m)
	return m


func test_starts_on_main_panel() -> void:
	var m := _main()
	assert_eq(m.current_panel(), "main")


func test_new_game_button_opens_new_game_and_back_returns() -> void:
	var m := _main()
	m.get_node("%NewGameButton").pressed.emit()
	assert_eq(m.current_panel(), "new_game")
	assert_true(m.get_node("%NewGameMenu").visible)
	assert_false(m.get_node("%MainPanel").visible)
	m.get_node("%NewGameMenu").get_node("%BackButton").pressed.emit()
	assert_eq(m.current_panel(), "main")
	assert_true(m.get_node("%MainPanel").visible)


func test_settings_button_opens_settings_and_back_returns() -> void:
	var m := _main()
	m.get_node("%SettingsButton").pressed.emit()
	assert_eq(m.current_panel(), "settings")
	assert_true(m.get_node("%SettingsMenu").visible)
	m.get_node("%SettingsMenu").get_node("%BackButton").pressed.emit()
	assert_eq(m.current_panel(), "main")


func test_quit_button_calls_quit_handler() -> void:
	var m := _main()
	var quits: Array = []
	m.quit_handler = func() -> void: quits.append(true)
	m.get_node("%QuitButton").pressed.emit()
	assert_eq(quits.size(), 1)


func test_start_hands_economy_and_seed_to_start_handler() -> void:
	var m := _main()
	var starts: Array = []
	m.start_handler = func(economy: String, game_seed: int) -> void: starts.append([economy, game_seed])
	m.get_node("%NewGameButton").pressed.emit()
	var ng: Control = m.get_node("%NewGameMenu")
	ng.set_seed(1234)
	ng.get_node("%StartButton").pressed.emit()
	assert_eq(starts, [["", 1234]])


func test_new_game_random_seed_changes_seed() -> void:
	var ng: Control = NewGameScene.instantiate()
	add_child_autofree(ng)
	ng.set_seed(5)
	ng.get_node("%RandomButton").pressed.emit()
	assert_ne(ng.get_seed(), 5)
	assert_true(ng.get_seed() >= 0)


func test_new_game_from_file_needs_a_file_before_start() -> void:
	var ng: Control = NewGameScene.instantiate()
	add_child_autofree(ng)
	watch_signals(ng)
	ng.select_economy_source(1)
	assert_true(ng.get_node("%StartButton").disabled, "no file picked yet")
	ng.use_economy_file("/tmp/economy.yaml")
	assert_false(ng.get_node("%StartButton").disabled)
	ng.set_seed(9)
	ng.get_node("%StartButton").pressed.emit()
	assert_signal_emitted_with_parameters(ng, "start_requested", ["/tmp/economy.yaml", 9])


func test_new_game_shipped_economy_is_empty_path() -> void:
	var ng: Control = NewGameScene.instantiate()
	add_child_autofree(ng)
	ng.use_economy_file("/tmp/economy.yaml")
	ng.select_economy_source(0)
	assert_eq(ng.economy_path(), "")


func test_settings_tabs_and_controls_list_bindings() -> void:
	var sm: Control = SettingsScene.instantiate()
	add_child_autofree(sm)
	var tabs: TabContainer = sm.get_node("%Tabs")
	assert_eq(tabs.get_tab_count(), 3)
	assert_eq(tabs.get_tab_title(0), "Graphics")
	assert_eq(tabs.get_tab_title(1), "Controls")
	assert_eq(tabs.get_tab_title(2), "Camera")
	var text: String = sm.bindings_text()
	assert_string_contains(text, "pause_toggle")
	assert_string_contains(text, "Space")
	assert_string_contains(text, "menu")
	assert_false(text.contains("ui_accept"), "engine ui_* actions are not listed")


func test_settings_widgets_write_through_to_settings() -> void:
	var s: Node = SettingsScript.new()
	s.path = "user://test_settings_menu.cfg"
	add_child_autofree(s)
	var sm: Control = SettingsScene.instantiate()
	sm.settings = s
	add_child_autofree(sm)
	sm.get_node("%InvertRotateCheck").toggled.emit(true)
	sm.get_node("%PanSpeedSlider").value_changed.emit(90.0)
	assert_eq(s.get_value("invert_rotate"), true)
	assert_almost_eq(float(s.get_value("pan_speed")), 90.0, 0.001)
	DirAccess.remove_absolute("user://test_settings_menu.cfg")
