extends PanelContainer
## Settings sub-menu (Graphics, Controls, Camera). Used from the main menu and the in-game menu.
## Writes straight through to the Settings autoload, which saves and emits `changed`.

signal back_requested

const MSAA_LABELS := ["Off", "2×", "4×", "8×"]

## Injected in tests; defaults to the `Settings` autoload.
var settings: Node

@onready var _fullscreen: BaseButton = %FullscreenCheck
@onready var _vsync: BaseButton = %VsyncCheck
@onready var _msaa: OptionButton = %MsaaOption
@onready var _pan_speed: Range = %PanSpeedSlider
@onready var _pan_value: Label = %PanSpeedValue
@onready var _edge_pan: BaseButton = %EdgePanCheck
@onready var _invert: BaseButton = %InvertRotateCheck


func _ready() -> void:
	if settings == null:
		settings = get_node_or_null("/root/Settings")
	for i in MSAA_LABELS.size():
		_msaa.add_item(MSAA_LABELS[i], i)
	_fullscreen.toggled.connect(_store.bind("fullscreen"))
	_vsync.toggled.connect(_store.bind("vsync"))
	_msaa.item_selected.connect(_store.bind("msaa"))
	_pan_speed.value_changed.connect(_store.bind("pan_speed"))
	_edge_pan.toggled.connect(_store.bind("edge_pan"))
	_invert.toggled.connect(_store.bind("invert_rotate"))
	%BackButton.pressed.connect(back_requested.emit)
	visibility_changed.connect(refresh)
	_build_bindings()
	refresh()


## Reads every widget's value back from Settings.
func refresh() -> void:
	if settings == null or not is_node_ready():
		return
	_fullscreen.set_pressed_no_signal(settings.get_value("fullscreen"))
	_vsync.set_pressed_no_signal(settings.get_value("vsync"))
	_msaa.select(settings.get_value("msaa"))
	_pan_speed.set_value_no_signal(settings.get_value("pan_speed"))
	_pan_value.text = "%d" % settings.get_value("pan_speed")
	_edge_pan.set_pressed_no_signal(settings.get_value("edge_pan"))
	_invert.set_pressed_no_signal(settings.get_value("invert_rotate"))


## One line per game action, "action: binding, binding", from the live InputMap.
func bindings_text() -> String:
	var lines: PackedStringArray = []
	for action: StringName in _game_actions():
		lines.append("%s: %s" % [action, ", ".join(_bindings(action))])
	return "\n".join(lines)


func _store(value: Variant, key: String) -> void:
	if settings != null:
		settings.set_value(key, value)
	if key == "pan_speed":
		_pan_value.text = "%d" % value


func _build_bindings() -> void:
	var grid: GridContainer = %BindingsList
	for action: StringName in _game_actions():
		var name_label := Label.new()
		name_label.text = String(action).capitalize()
		name_label.tooltip_text = action
		name_label.mouse_filter = Control.MOUSE_FILTER_PASS
		var keys_label := Label.new()
		keys_label.text = ", ".join(_bindings(action))
		grid.add_child(name_label)
		grid.add_child(keys_label)


static func _game_actions() -> Array[StringName]:
	var out: Array[StringName] = []
	for action: StringName in InputMap.get_actions():
		if not String(action).begins_with("ui_"):
			out.append(action)
	out.sort()
	return out


static func _bindings(action: StringName) -> PackedStringArray:
	var out: PackedStringArray = []
	for ev: InputEvent in InputMap.action_get_events(action):
		out.append(_event_text(ev))
	return out


static func _event_text(ev: InputEvent) -> String:
	if ev is InputEventKey:
		var key := ev as InputEventKey
		var code := key.keycode if key.keycode != KEY_NONE else key.physical_keycode
		return OS.get_keycode_string(code)
	if ev is InputEventMouseButton:
		match (ev as InputEventMouseButton).button_index:
			MOUSE_BUTTON_LEFT: return "Left mouse"
			MOUSE_BUTTON_RIGHT: return "Right mouse"
			MOUSE_BUTTON_MIDDLE: return "Middle mouse"
			MOUSE_BUTTON_WHEEL_UP: return "Wheel up"
			MOUSE_BUTTON_WHEEL_DOWN: return "Wheel down"
	return ev.as_text()
