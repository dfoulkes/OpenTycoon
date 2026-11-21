extends PanelContainer
## Corner widget: pause and the four speed steps. It only asks; the game controller decides.

signal speed_requested(speed: int)
signal pause_toggle_requested

@onready var _buttons: Array[Button] = [%PauseButton, %Speed1Button, %Speed2Button, %Speed3Button, %Speed4Button]


func _ready() -> void:
	_buttons[0].pressed.connect(pause_toggle_requested.emit)
	for speed in range(1, 5):
		_buttons[speed].pressed.connect(speed_requested.emit.bind(speed))
	show_speed(1)


## Marks the running speed (0 = paused).
func show_speed(speed: int) -> void:
	for i in _buttons.size():
		_buttons[i].set_pressed_no_signal(i == speed)
