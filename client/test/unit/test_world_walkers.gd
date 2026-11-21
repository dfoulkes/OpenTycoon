extends GutTest
## Walkers: one MultiMesh fed from the flat [x0, z0, x1, z1, ...] array, lerped between deltas.

const Walkers := preload("res://scripts/world/walkers.gd")

var crowd: Walkers


func before_each() -> void:
	crowd = Walkers.new()
	add_child_autofree(crowd)


func test_count_matches_the_flat_array() -> void:
	crowd.set_positions([1.0, 2.0, 3.0, 4.0])
	assert_eq(crowd.count(), 2)
	assert_eq(crowd.multimesh.visible_instance_count, 2)


func test_an_odd_trailing_value_is_ignored() -> void:
	crowd.set_positions([1.0, 2.0, 3.0])
	assert_eq(crowd.count(), 1)


func test_first_positions_are_drawn_where_they_are() -> void:
	crowd.set_positions([10.0, 20.0])
	assert_eq(crowd.drawn_position(0), Vector2(10, 20))


func test_walkers_lerp_from_where_they_were_to_the_new_position() -> void:
	crowd.set_positions([0.0, 0.0])
	crowd.set_positions([10.0, 0.0])
	assert_eq(crowd.drawn_position(0), Vector2(0, 0), "starts at the old position")
	crowd.advance(crowd.interval() / 2.0)
	assert_almost_eq(crowd.drawn_position(0).x, 5.0, 0.001)
	crowd.advance(crowd.interval())
	assert_eq(crowd.drawn_position(0), Vector2(10, 0), "and stops at the new one")


func test_a_change_in_count_snaps_instead_of_lerping() -> void:
	crowd.set_positions([0.0, 0.0])
	crowd.set_positions([10.0, 0.0, 20.0, 0.0])
	assert_eq(crowd.drawn_position(0), Vector2(10, 0))
	assert_eq(crowd.drawn_position(1), Vector2(20, 0))


func test_twenty_thousand_walkers_fit_and_update_quickly() -> void:
	var flat := PackedFloat32Array()
	flat.resize(40000)
	for i in 20000:
		flat[i * 2] = float(i % 300)
		flat[i * 2 + 1] = floorf(i / 300.0)
	crowd.set_positions(flat)
	var started := Time.get_ticks_usec()
	crowd.set_positions(flat)
	var took_ms := (Time.get_ticks_usec() - started) / 1000.0
	assert_eq(crowd.count(), 20000)
	assert_lt(took_ms, 100.0, "one update of 20k walkers took %.1f ms" % took_ms)
