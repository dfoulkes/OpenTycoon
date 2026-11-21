extends Node3D
## One lot on the board (§10d #3, #4): who owns it, its building and, for the city, a flag.
##
## The node sits at the block's centre on the ground. An owned lot gets a pad in the owner's
## colour inset in the block; its building is a stand-in kit model tinted the same colour, as
## tall as its floors. City lots use the civic colour and carry a flag so city businesses stand
## out. Unowned land shows only the block. A pick body covers the lot for `select`.

const Palette := preload("res://scripts/world/palette.gd")
const MODELS: Array[String] = [
	"res://assets/kenney/starter-kit-city-builder/building-small-a.glb",
	"res://assets/kenney/starter-kit-city-builder/building-small-b.glb",
	"res://assets/kenney/starter-kit-city-builder/building-small-c.glb",
	"res://assets/kenney/starter-kit-city-builder/building-small-d.glb",
]
const COLORMAP := "res://assets/kenney/starter-kit-city-builder/Textures/colormap.png"
## Share of the block the owner pad covers, and the building's footprint.
const PAD_SHARE := 0.86
const BUILDING_SHARE := 0.6
const PAD_HEIGHT := 0.25
const BLOCK_TOP := 0.3
## Collision layer for lot picking.
const PICK_LAYER := 1 << 1

var key := ""
var owner_id := "unowned"
var _size := Vector2.ONE
var _floors := 0
var _pad: MeshInstance3D
var _building: Node3D
var _flag: Node3D
var _pick_shape: CollisionShape3D


## Places the lot on its block (`board.blocks[]` from the snapshot).
func setup(block: Dictionary) -> void:
	key = str(block.get("key", ""))
	name = "Lot_%s" % key
	_size = Vector2(float(block.get("w", 1.0)), float(block.get("d", 1.0)))
	position = Vector3(
		float(block.get("x", 0.0)) + _size.x / 2.0, 0.0,
		float(block.get("z", 0.0)) + _size.y / 2.0)
	_build_pick_body()


## Shows a lot (the protocol's `lot` shape): owner, building and city marker.
func apply(lot: Dictionary) -> void:
	owner_id = str(lot.get("owner", "unowned"))
	var building: Variant = lot.get("building")
	_floors = int((building as Dictionary).get("floors", 1)) if building is Dictionary else 0
	_show_pad()
	_show_building()
	_show_flag()
	_fit_pick_body()


func owner_color() -> Color:
	return Palette.owner_color(owner_id)


func has_owner_pad() -> bool:
	return _pad != null


func has_building() -> bool:
	return _building != null


func has_city_marker() -> bool:
	return _flag != null


## The building's height in metres (0 with no building).
func building_height() -> float:
	return _floors * Palette.FLOOR_HEIGHT_M if _floors > 0 else 0.0


## The lot's footprint (x, z) in metres.
func footprint() -> Vector2:
	return _size


## The top of whatever stands on the lot, in metres.
func top() -> float:
	return BLOCK_TOP + building_height()


func _show_pad() -> void:
	_free(_pad)
	_pad = null
	if owner_id == "unowned":
		return
	_pad = MeshInstance3D.new()
	_pad.name = "OwnerPad"
	var box := BoxMesh.new()
	box.size = Vector3(_size.x * PAD_SHARE, PAD_HEIGHT, _size.y * PAD_SHARE)
	_pad.mesh = box
	_pad.material_override = Palette.material(owner_color())
	_pad.position.y = BLOCK_TOP + PAD_HEIGHT / 2.0
	add_child(_pad)


func _show_building() -> void:
	_free(_building)
	_building = null
	if _floors <= 0:
		return
	_building = Node3D.new()
	_building.name = "Building"
	var side := minf(_size.x, _size.y) * BUILDING_SHARE
	var target := Vector3(side, building_height(), side)
	var model := _model_for(key)
	if model == null:
		model = _box_model()
	var bounds := _bounds_of(model)
	model.position = -Vector3(bounds.get_center().x, bounds.position.y, bounds.get_center().z)
	_tint(model, owner_color())
	_building.add_child(model)
	_building.scale = target / bounds.size.max(Vector3.ONE * 0.001)
	_building.position.y = BLOCK_TOP + (PAD_HEIGHT if _pad != null else 0.0)
	add_child(_building)


func _show_flag() -> void:
	_free(_flag)
	_flag = null
	if owner_id != "city":
		return
	_flag = Node3D.new()
	_flag.name = "CityFlag"
	var pole_height := maxf(top() + 6.0, 10.0)
	var pole := MeshInstance3D.new()
	var pole_mesh := CylinderMesh.new()
	pole_mesh.top_radius = 0.3
	pole_mesh.bottom_radius = 0.3
	pole_mesh.height = pole_height
	pole.mesh = pole_mesh
	pole.material_override = Palette.material(Color(0.9, 0.9, 0.9))
	pole.position.y = pole_height / 2.0
	_flag.add_child(pole)
	var cloth := MeshInstance3D.new()
	var cloth_mesh := BoxMesh.new()
	cloth_mesh.size = Vector3(5.0, 3.2, 0.2)
	cloth.mesh = cloth_mesh
	cloth.material_override = Palette.marker_material(Palette.CIVIC)
	cloth.position = Vector3(2.6, pole_height - 1.8, 0.0)
	_flag.add_child(cloth)
	_flag.position = Vector3(-_size.x * 0.38, BLOCK_TOP, -_size.y * 0.38)
	add_child(_flag)


func _build_pick_body() -> void:
	var body := StaticBody3D.new()
	body.name = "Pick"
	body.collision_layer = PICK_LAYER
	body.collision_mask = 0
	body.set_meta("lot_key", key)
	_pick_shape = CollisionShape3D.new()
	_pick_shape.shape = BoxShape3D.new()
	body.add_child(_pick_shape)
	add_child(body)
	_fit_pick_body()


func _fit_pick_body() -> void:
	var height := maxf(top(), BLOCK_TOP)
	(_pick_shape.shape as BoxShape3D).size = Vector3(_size.x, height, _size.y)
	_pick_shape.position.y = height / 2.0


static func _model_for(lot_key: String) -> Node3D:
	var path := MODELS[absi(lot_key.hash()) % MODELS.size()]
	if not ResourceLoader.exists(path):
		return null
	var scene := load(path) as PackedScene
	return scene.instantiate() as Node3D if scene != null else null


## Grey-box stand-in when a kit model is missing.
static func _box_model() -> Node3D:
	var box := MeshInstance3D.new()
	var mesh := BoxMesh.new()
	box.mesh = mesh
	return box


## The model's bounds in its own space, from its meshes.
static func _bounds_of(model: Node3D) -> AABB:
	var bounds := AABB()
	var first := true
	for mesh_instance in _meshes(model):
		var local := _transform_to(model, mesh_instance) * mesh_instance.mesh.get_aabb()
		bounds = local if first else bounds.merge(local)
		first = false
	return bounds if not first else AABB(Vector3(-0.5, 0, -0.5), Vector3.ONE)


static func _tint(model: Node3D, color: Color) -> void:
	var texture := load(COLORMAP) as Texture2D if ResourceLoader.exists(COLORMAP) else null
	for mesh_instance in _meshes(model):
		mesh_instance.material_override = (
			Palette.tinted_material(texture, color) if texture != null
			else Palette.material(color))


static func _meshes(root: Node) -> Array[MeshInstance3D]:
	var found: Array[MeshInstance3D] = []
	if root is MeshInstance3D and (root as MeshInstance3D).mesh != null:
		found.append(root)
	for child in root.get_children():
		found.append_array(_meshes(child))
	return found


static func _transform_to(root: Node3D, node: Node3D) -> Transform3D:
	var xform := Transform3D.IDENTITY
	var current: Node = node
	while current != root and current is Node3D:
		xform = (current as Node3D).transform * xform
		current = current.get_parent()
	return xform


static func _free(node: Node) -> void:
	if node != null:
		node.get_parent().remove_child(node)
		node.free()
