extends PanelContainer
## New Game sub-menu: pick the economy (shipped or an `economy.yaml` from disk) and the seed.

signal start_requested(economy_path: String, game_seed: int)
signal back_requested

const SHIPPED := 0
const FROM_FILE := 1

var _file := ""

@onready var _economy: OptionButton = %EconomyOption
@onready var _file_row: Control = %FileRow
@onready var _file_label: Label = %FileLabel
@onready var _seed_edit: LineEdit = %SeedEdit
@onready var _start: Button = %StartButton
@onready var _dialog: FileDialog = %FileDialog


func _ready() -> void:
	_economy.clear()
	_economy.add_item("Shipped", SHIPPED)
	_economy.add_item("From file…", FROM_FILE)
	_economy.item_selected.connect(select_economy_source)
	%BrowseButton.pressed.connect(_dialog.popup_centered_ratio.bind(0.6))
	_dialog.file_selected.connect(use_economy_file)
	%RandomButton.pressed.connect(randomise_seed)
	%BackButton.pressed.connect(back_requested.emit)
	_start.pressed.connect(func() -> void: start_requested.emit(economy_path(), get_seed()))
	_seed_edit.text_changed.connect(func(_t: String) -> void: _refresh())
	randomise_seed()
	select_economy_source(SHIPPED)


func select_economy_source(index: int) -> void:
	_economy.select(index)
	_refresh()


func use_economy_file(path: String) -> void:
	_file = path
	_file_label.text = path.get_file() if path != "" else "No file chosen"
	_file_label.tooltip_text = path
	_refresh()


## The economy to load: "" means the shipped one.
func economy_path() -> String:
	return _file if _economy.selected == FROM_FILE else ""


func get_seed() -> int:
	return _seed_edit.text.to_int()


func set_seed(value: int) -> void:
	_seed_edit.text = str(value)
	_refresh()


func randomise_seed() -> void:
	var old := get_seed()
	var value := old
	while value == old:
		value = (randi() << 16) ^ randi()
	set_seed(value)


func _refresh() -> void:
	_file_row.visible = _economy.selected == FROM_FILE
	var has_economy := _economy.selected == SHIPPED or _file != ""
	_start.disabled = not (has_economy and _seed_edit.text.is_valid_int() and get_seed() >= 0)
