extends MultiMeshInstance3D
## Walkers (§10d #5): every shopper on the streets as one MultiMesh of small capsules.
##
## Each delta carries the full flat array `[x0, z0, x1, z1, ...]` in metres
## (docs/inspector-protocol.md). A walker is drawn from where it is now towards its new
## position over the time between deltas. The CPU writes the buffer once per delta (four floats
## per walker); the per-frame lerp runs in `walker.gdshader`, so 20,000 walkers cost one draw
## call and a shader parameter per frame. No ids in v1: when the count changes, walkers snap.

const SHADER := preload("res://scripts/world/walker.gdshader")
## Floats per instance in the MultiMesh buffer: a 3×4 transform then 4 custom floats.
const STRIDE := 16
const ORIGIN_X := 3
const ORIGIN_Y := 7
const ORIGIN_Z := 11
const MOVE_X := 12
const MOVE_Z := 13

const WALKER_RADIUS := 0.35
const WALKER_HEIGHT := 1.7
## Seconds a move takes until the server's real cadence has been measured.
const DEFAULT_INTERVAL := 0.25

var _from := PackedFloat32Array()
var _to := PackedFloat32Array()
var _buffer := PackedFloat32Array()
var _t := 1.0
var _interval := DEFAULT_INTERVAL
var _last_update_usec := 0
var _material := ShaderMaterial.new()


func _init() -> void:
	cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	_material.shader = SHADER
	var capsule := CapsuleMesh.new()
	capsule.radius = WALKER_RADIUS
	capsule.height = WALKER_HEIGHT
	capsule.radial_segments = 6
	capsule.rings = 1
	capsule.material = _material
	multimesh = MultiMesh.new()
	multimesh.transform_format = MultiMesh.TRANSFORM_3D
	multimesh.use_custom_data = true
	multimesh.mesh = capsule
	multimesh.visible_instance_count = 0


func _process(delta: float) -> void:
	advance(delta)


## Replaces every walker's target with the flat `[x, z, ...]` array (an Array or packed array).
func set_positions(flat: Variant) -> void:
	var target := PackedFloat32Array(flat)
	if target.size() % 2 == 1:
		target.resize(target.size() - 1)
	@warning_ignore("integer_division")
	var n := target.size() / 2
	_measure_interval()
	var from := target
	if target.size() == _to.size():
		from = _drawn_now()
	_from = from
	_to = target
	_t = 1.0 if from == target else 0.0
	_write_buffer(n)
	_material.set_shader_parameter("lerp_t", _t)


## How many walkers are drawn.
func count() -> int:
	return multimesh.visible_instance_count


## Where walker `index` is drawn this frame, as (x, z).
func drawn_position(index: int) -> Vector2:
	var i := index * 2
	return Vector2(lerpf(_from[i], _to[i], _t), lerpf(_from[i + 1], _to[i + 1], _t))


## Seconds one move takes: the measured time between the last two updates.
func interval() -> float:
	return _interval


## Moves the lerp on by `seconds`.
func advance(seconds: float) -> void:
	if _t >= 1.0:
		return
	_t = minf(1.0, _t + seconds / _interval)
	_material.set_shader_parameter("lerp_t", _t)


func _drawn_now() -> PackedFloat32Array:
	if _t >= 1.0:
		return _to
	var now := PackedFloat32Array()
	now.resize(_to.size())
	for i in _to.size():
		now[i] = lerpf(_from[i], _to[i], _t)
	return now


func _measure_interval() -> void:
	var now := Time.get_ticks_usec()
	if _last_update_usec > 0:
		_interval = clampf((now - _last_update_usec) / 1_000_000.0, 0.05, 2.0)
	_last_update_usec = now


func _write_buffer(n: int) -> void:
	_ensure_capacity(n)
	var y := WALKER_HEIGHT / 2.0
	for w in n:
		var b := w * STRIDE
		var i := w * 2
		_buffer[b + ORIGIN_X] = _from[i]
		_buffer[b + ORIGIN_Y] = y
		_buffer[b + ORIGIN_Z] = _from[i + 1]
		_buffer[b + MOVE_X] = _to[i] - _from[i]
		_buffer[b + MOVE_Z] = _to[i + 1] - _from[i + 1]
	multimesh.buffer = _buffer
	multimesh.visible_instance_count = n


## Grows the MultiMesh in powers of two; identity bases are written once per growth.
func _ensure_capacity(n: int) -> void:
	if n <= multimesh.instance_count:
		return
	var capacity := maxi(64, nearest_po2(n))
	multimesh.instance_count = capacity
	_buffer = PackedFloat32Array()
	_buffer.resize(capacity * STRIDE)
	for w in capacity:
		var b := w * STRIDE
		_buffer[b] = 1.0
		_buffer[b + 5] = 1.0
		_buffer[b + 10] = 1.0
