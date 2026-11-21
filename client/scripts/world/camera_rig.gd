extends Node3D
## Bird's-eye camera rig (stub).

@export var edge_pan_enabled := true
@export var invert_zoom := false
@export var min_distance := 25.0
@export var max_distance := 600.0
var distance := 200.0


func set_bounds(_rect: Rect2) -> void:
	pass


func focus() -> Vector2:
	return Vector2(-1, -1)


func focus_on(_point: Vector2) -> void:
	pass


func pan_by(_direction: Vector2) -> void:
	pass


func rotate_by(_radians: float) -> void:
	pass


func zoom_by(_steps: float) -> void:
	pass


func camera() -> Camera3D:
	return $Camera3D
