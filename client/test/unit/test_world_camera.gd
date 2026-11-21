extends GutTest
## The bird's-eye camera rig: pans, zooms and rotates, and never leaves the board.

const CameraRig := preload("res://scripts/world/camera_rig.gd")
const BOARD := Rect2(0, 0, 124, 124)

var rig: CameraRig


func before_each() -> void:
	var scene: PackedScene = load("res://scenes/world/world.tscn")
	var world: Node3D = scene.instantiate()
	add_child_autofree(world)
	rig = world.camera_rig()
	rig.edge_pan_enabled = false
	rig.set_bounds(BOARD)


func test_focus_starts_inside_the_board() -> void:
	assert_true(BOARD.has_point(rig.focus()))


func test_panning_is_clamped_to_the_board() -> void:
	rig.pan_by(Vector2(100000, 0))
	assert_almost_eq(rig.focus().x, BOARD.end.x, 0.001)
	rig.pan_by(Vector2(-100000, 0))
	assert_almost_eq(rig.focus().x, BOARD.position.x, 0.001)
	rig.pan_by(Vector2(0, 100000))
	assert_true(BOARD.grow(0.001).has_point(rig.focus()))
	rig.pan_by(Vector2(0, -100000))
	assert_true(BOARD.grow(0.001).has_point(rig.focus()))


func test_panning_forward_follows_the_camera_heading() -> void:
	rig.focus_on(BOARD.get_center())
	rig.pan_by(Vector2(0, 10))
	assert_lt(rig.focus().y, BOARD.get_center().y, "unrotated, forward is north (-z)")
	rig.focus_on(BOARD.get_center())
	rig.rotate_by(PI / 2)
	rig.pan_by(Vector2(0, 10))
	assert_almost_eq(rig.focus().y, BOARD.get_center().y, 0.001)
	assert_ne(rig.focus().x, BOARD.get_center().x)


func test_new_bounds_pull_the_focus_back_inside() -> void:
	rig.focus_on(Vector2(120, 120))
	rig.set_bounds(Rect2(0, 0, 50, 50))
	assert_true(Rect2(0, 0, 50, 50).grow(0.001).has_point(rig.focus()))


func test_zoom_is_clamped() -> void:
	rig.zoom_by(1000)
	assert_almost_eq(rig.distance, rig.max_distance, 0.001)
	rig.zoom_by(-1000)
	assert_almost_eq(rig.distance, rig.min_distance, 0.001)


func test_invert_zoom_flips_the_wheel() -> void:
	var before := rig.distance
	rig.invert_zoom = true
	rig.zoom_by(1)
	assert_lt(rig.distance, before)


func test_camera_looks_at_the_focus() -> void:
	rig.focus_on(Vector2(30, 40))
	var cam: Camera3D = rig.camera()
	var forward := -cam.global_transform.basis.z
	var to_focus := (Vector3(30, 0, 40) - cam.global_position).normalized()
	assert_almost_eq(forward.dot(to_focus), 1.0, 0.001)


func test_camera_stays_above_the_ground() -> void:
	rig.zoom_by(-1000)
	assert_gt(rig.camera().global_position.y, 0.0)
