extends Node3D
## A glowing frame around the selected lot.

const Palette := preload("res://scripts/world/palette.gd")
const BAR_WIDTH := 1.2
const BAR_HEIGHT := 0.8

var lot_key := ""
var _bars: Array[MeshInstance3D] = []


func _init() -> void:
	visible = false
	for i in 4:
		var bar := MeshInstance3D.new()
		bar.mesh = BoxMesh.new()
		bar.material_override = Palette.marker_material(Palette.HIGHLIGHT, 0.85)
		bar.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
		_bars.append(bar)
		add_child(bar)


## Frames a lot `size` metres across (x, z) centred on `center`.
func frame(key: String, center: Vector3, size: Vector2) -> void:
	lot_key = key
	position = Vector3(center.x, BAR_HEIGHT / 2.0 + 0.3, center.z)
	var half := size / 2.0 + Vector2.ONE * BAR_WIDTH / 2.0
	var outer := size + Vector2.ONE * BAR_WIDTH * 2.0
	_place(_bars[0], Vector3(0, 0, -half.y), Vector3(outer.x, BAR_HEIGHT, BAR_WIDTH))
	_place(_bars[1], Vector3(0, 0, half.y), Vector3(outer.x, BAR_HEIGHT, BAR_WIDTH))
	_place(_bars[2], Vector3(-half.x, 0, 0), Vector3(BAR_WIDTH, BAR_HEIGHT, size.y))
	_place(_bars[3], Vector3(half.x, 0, 0), Vector3(BAR_WIDTH, BAR_HEIGHT, size.y))
	visible = true


func clear() -> void:
	lot_key = ""
	visible = false


static func _place(bar: MeshInstance3D, at: Vector3, size: Vector3) -> void:
	bar.position = at
	(bar.mesh as BoxMesh).size = size
