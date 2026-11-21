extends Node3D
## Bird's-eye RTS camera (§10d #2): pan, zoom and rotate over the board, never off it.
##
## The rig sits on the ground at the focus point and turns about Y (the heading). Its
## `Camera3D` child sits `distance` metres back along a fixed pitch, looking at the focus.
## Input uses the actions in `project.godot`: `cam_pan_*` (keys), `cam_rotate_*` (Q/E),
## `cam_zoom_*` (wheel) and `cam_drag_rotate` (middle-drag). The settings menu sets the
## exported tunables directly.

## The distance at which panning moves at `pan_speed`.
const REFERENCE_DISTANCE := 200.0

## Metres per second at the reference distance; panning speeds up as the camera zooms out.
@export var pan_speed := 120.0
## Pan when the mouse touches the window edge.
@export var edge_pan_enabled := true
## How close to the edge, in pixels, the mouse must be to edge-pan.
@export var edge_margin_px := 12.0
@export var invert_pan := false
@export var invert_zoom := false
@export var invert_rotate := false
## Radians per second for Q/E.
@export var rotate_speed := 1.6
## Radians per pixel of middle-drag.
@export var drag_rotate_sensitivity := 0.008
## Each wheel step scales the distance by (1 + zoom_step).
@export var zoom_step := 0.12
@export var min_distance := 25.0
@export var max_distance := 600.0
## Camera pitch below the horizon, in degrees.
@export_range(20.0, 89.0) var pitch_degrees := 55.0:
	set(value):
		pitch_degrees = value
		_place_camera()

## Distance from the camera to the focus, in metres.
var distance := 220.0:
	set(value):
		distance = clampf(value, min_distance, max_distance)
		_place_camera()

var _bounds := Rect2()
var _dragging := false


func _ready() -> void:
	_place_camera()


func _process(delta: float) -> void:
	var move := Input.get_vector("cam_pan_left", "cam_pan_right", "cam_pan_down", "cam_pan_up")
	if edge_pan_enabled:
		move += _edge_direction()
	if move != Vector2.ZERO:
		pan_by(move.limit_length(1.0) * pan_speed * delta * distance / REFERENCE_DISTANCE)
	var turn := Input.get_axis("cam_rotate_left", "cam_rotate_right")
	if turn != 0.0:
		rotate_by(turn * rotate_speed * delta * (-1.0 if invert_rotate else 1.0))


func _unhandled_input(event: InputEvent) -> void:
	if event.is_action_pressed("cam_zoom_in"):
		zoom_by(-1.0)
	elif event.is_action_pressed("cam_zoom_out"):
		zoom_by(1.0)
	elif event is InputEventMouseButton and event.is_action("cam_drag_rotate"):
		_dragging = event.is_pressed()
	elif event is InputEventMouseMotion and _dragging:
		var way := -1.0 if invert_rotate else 1.0
		rotate_by(-(event as InputEventMouseMotion).relative.x * drag_rotate_sensitivity * way)


## Keeps the focus inside `rect` (x, z on the ground), pulling it back in if it is outside.
func set_bounds(rect: Rect2) -> void:
	_bounds = rect
	focus_on(focus())


## The focus point on the ground, as (x, z).
func focus() -> Vector2:
	return Vector2(position.x, position.z)


## Moves the focus to (x, z), clamped to the bounds.
func focus_on(point: Vector2) -> void:
	if _bounds.has_area():
		point = point.clamp(_bounds.position, _bounds.end)
	position = Vector3(point.x, 0.0, point.y)


## Moves the focus by `offset` metres relative to the heading: x is right, y is forward.
func pan_by(offset: Vector2) -> void:
	if invert_pan:
		offset = -offset
	var right := Vector2(cos(rotation.y), -sin(rotation.y))
	var forward := Vector2(-sin(rotation.y), -cos(rotation.y))
	focus_on(focus() + right * offset.x + forward * offset.y)


## Turns the heading by `radians` (positive is anticlockwise seen from above).
func rotate_by(radians: float) -> void:
	rotation.y = wrapf(rotation.y + radians, -PI, PI)


## Zooms out by `steps` wheel steps (negative zooms in), clamped.
func zoom_by(steps: float) -> void:
	if invert_zoom:
		steps = -steps
	distance = distance * pow(1.0 + zoom_step, steps)


func camera() -> Camera3D:
	return $Camera3D


func _place_camera() -> void:
	var cam := get_node_or_null("Camera3D") as Camera3D
	if cam == null:
		return
	var pitch := deg_to_rad(pitch_degrees)
	cam.position = Vector3(0.0, sin(pitch) * distance, cos(pitch) * distance)
	cam.rotation = Vector3(-pitch, 0.0, 0.0)
	cam.far = maxf(cam.far, max_distance * 4.0)


## A unit-ish direction from the mouse touching a window edge, or zero.
func _edge_direction() -> Vector2:
	if DisplayServer.get_name() == "headless" or not get_window().has_focus():
		return Vector2.ZERO
	var viewport := get_viewport()
	var rect := viewport.get_visible_rect()
	var mouse := viewport.get_mouse_position()
	if not rect.has_point(mouse):
		return Vector2.ZERO
	var dir := Vector2.ZERO
	if mouse.x < edge_margin_px:
		dir.x = -1.0
	elif mouse.x > rect.size.x - edge_margin_px:
		dir.x = 1.0
	if mouse.y < edge_margin_px:
		dir.y = 1.0
	elif mouse.y > rect.size.y - edge_margin_px:
		dir.y = -1.0
	return dir
