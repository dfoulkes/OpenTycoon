extends Control
## In-game menu: Continue, Settings, Quit to Main Menu, Quit. Pausing is the controller's job.

signal continue_requested
signal quit_to_menu_requested
signal quit_requested


func _ready() -> void:
	%ContinueButton.pressed.connect(continue_requested.emit)
	%SettingsButton.pressed.connect(_show_settings.bind(true))
	%QuitToMenuButton.pressed.connect(quit_to_menu_requested.emit)
	%QuitButton.pressed.connect(quit_requested.emit)
	%SettingsMenu.back_requested.connect(_show_settings.bind(false))
	close()


func open() -> void:
	_show_settings(false)
	show()
	%ContinueButton.grab_focus.call_deferred()


func close() -> void:
	hide()


func is_open() -> bool:
	return visible


func _show_settings(on: bool) -> void:
	%SettingsMenu.visible = on
	%Buttons.visible = not on
