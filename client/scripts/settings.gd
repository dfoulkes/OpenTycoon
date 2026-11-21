extends Node
## Player settings, autoloaded as `Settings`. Persisted to `user://settings.cfg` and applied on
## load. Listeners (the world camera) connect to `changed` and read `get_value(key)`.

signal changed(key: String)

## Every setting, its ConfigFile section and default. MSAA uses Viewport.MSAA (0 off … 3 8×).
const DEFAULTS := {
	"fullscreen": ["graphics", false],
	"vsync": ["graphics", true],
	"msaa": ["graphics", 0],
	"pan_speed": ["camera", 40.0],
	"edge_pan": ["camera", true],
	"invert_rotate": ["camera", false],
}

var path := "user://settings.cfg"
var _values := {}


func _init() -> void:
	for key: String in DEFAULTS:
		_values[key] = DEFAULTS[key][1]


func _ready() -> void:
	if get_tree().root == get_parent():
		load_settings()


func get_value(key: String) -> Variant:
	return _values.get(key)


## Sets, applies and saves one setting, then emits `changed`. Unknown keys are ignored.
func set_value(key: String, value: Variant) -> void:
	if not DEFAULTS.has(key):
		return
	_values[key] = type_convert(value, typeof(DEFAULTS[key][1]))
	apply()
	save_settings()
	changed.emit(key)


func load_settings() -> void:
	var cfg := ConfigFile.new()
	if cfg.load(path) == OK:
		for key: String in DEFAULTS:
			var def: Variant = DEFAULTS[key][1]
			_values[key] = type_convert(cfg.get_value(DEFAULTS[key][0], key, def), typeof(def))
	apply()


func save_settings() -> Error:
	var cfg := ConfigFile.new()
	for key: String in DEFAULTS:
		cfg.set_value(DEFAULTS[key][0], key, _values[key])
	return cfg.save(path)


## Pushes the graphics settings to the window and viewport. Camera settings are read by the camera.
func apply() -> void:
	if not is_inside_tree() or DisplayServer.get_name() == "headless":
		return
	var mode := DisplayServer.WINDOW_MODE_FULLSCREEN if _values["fullscreen"] else DisplayServer.WINDOW_MODE_WINDOWED
	if DisplayServer.window_get_mode() != mode:
		DisplayServer.window_set_mode(mode)
	var vs := DisplayServer.VSYNC_ENABLED if _values["vsync"] else DisplayServer.VSYNC_DISABLED
	DisplayServer.window_set_vsync_mode(vs)
	get_viewport().msaa_3d = _values["msaa"] as Viewport.MSAA
