extends Control
## Front end: the main menu and its New Game and Settings sub-menus. Start swaps to the game scene.

const GAME_SCENE := "res://scenes/game/game.tscn"
const THEME_PATH := "res://theme/inspector_theme.tres"

## Overridable in tests so nothing quits or changes scene.
var quit_handler: Callable = func() -> void: get_tree().quit()
var start_handler: Callable = _start_game

@onready var _panels := {"main": %MainPanel, "new_game": %NewGameMenu, "settings": %SettingsMenu}


func _ready() -> void:
	if ResourceLoader.exists(THEME_PATH):
		theme = load(THEME_PATH)
	%NewGameButton.pressed.connect(show_panel.bind("new_game"))
	%SettingsButton.pressed.connect(show_panel.bind("settings"))
	%QuitButton.pressed.connect(func() -> void: quit_handler.call())
	%NewGameMenu.back_requested.connect(show_panel.bind("main"))
	%SettingsMenu.back_requested.connect(show_panel.bind("main"))
	%NewGameMenu.start_requested.connect(func(e: String, s: int) -> void: start_handler.call(e, s))
	%Status.text = greeting()
	show_panel("main")


func show_panel(panel: String) -> void:
	for key: String in _panels:
		_panels[key].visible = key == panel


func current_panel() -> String:
	for key: String in _panels:
		if _panels[key].visible:
			return key
	return ""


func _start_game(economy_path: String, game_seed: int) -> void:
	var game: Node = (load(GAME_SCENE) as PackedScene).instantiate()
	game.economy_path = economy_path
	game.game_seed = game_seed
	var tree := get_tree()
	tree.root.add_child(game)
	tree.current_scene = game
	queue_free()


## Greets through the Rust extension if it loaded, so a missing build is obvious on screen.
static func greeting() -> String:
	if not ClassDB.class_exists("Hello"):
		return "Rust extension not loaded: run `cargo build -p client-ext`"
	var hello: Object = ClassDB.instantiate("Hello")
	var text: String = hello.call("greet", "London")
	hello.free()
	return text
