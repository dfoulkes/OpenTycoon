extends Node3D
## The static board (§4, §10d #2): ground, blocks coloured by district, streets, the Thames
## and its bridges, built from the snapshot's `board` (docs/inspector-protocol.md).
##
## Every rect in `board` is `{x, z, w, d}` in metres with (x, z) its north-west corner, the
## same corner as the board's origin. Children: `Ground`, `Blocks` (one `Block_<key>` each,
## positioned at the block's centre), `Streets`, `River`, `Bridges`.

const Palette := preload("res://scripts/world/palette.gd")
const BLOCK_HEIGHT := 0.3
const STREET_HEIGHT := 0.08
const RIVER_TOP := 0.04
const BRIDGE_HEIGHT := 0.6
## Ground beyond the board's edge, in metres.
const MARGIN := 1500.0

var _blocks: Dictionary = {}


## Rebuilds everything from `board`.
func build(board: Dictionary) -> void:
	for child in get_children():
		remove_child(child)
		child.free()
	_blocks.clear()
	var size := Vector2(float(board.get("width_m", 0.0)), float(board.get("depth_m", 0.0)))
	_add_ground(size)
	var blocks := _group("Blocks")
	for entry: Dictionary in board.get("blocks", []):
		var color := Palette.district_color(str(entry.get("district", "")))
		if not bool(entry.get("buildable", true)):
			color = Palette.CLOSED_BLOCK
		var node := _slab(blocks, entry, BLOCK_HEIGHT, 0.0, color)
		node.name = "Block_%s" % str(entry.get("key", ""))
		_blocks[str(entry.get("key", ""))] = node
	_add_rects("Streets", board.get("streets", []), STREET_HEIGHT, 0.0, Palette.STREET)
	_add_rects("River", board.get("river", []), 0.2, RIVER_TOP - 0.2, Palette.RIVER)
	_add_rects("Bridges", board.get("bridges", []), BRIDGE_HEIGHT, 0.0, Palette.BRIDGE)


## The block node for `key`, or null.
func block(key: String) -> MeshInstance3D:
	return _blocks.get(key) as MeshInstance3D


func block_count() -> int:
	return _blocks.size()


## How many rects of a feature (`streets`, `river`, `bridges`) were drawn.
func feature_count(kind: String) -> int:
	var group := get_node_or_null(kind.capitalize())
	return group.get_child_count() if group != null else 0


func _add_ground(size: Vector2) -> void:
	var ground := MeshInstance3D.new()
	ground.name = "Ground"
	var plane := PlaneMesh.new()
	plane.size = size + Vector2.ONE * MARGIN * 2.0
	ground.mesh = plane
	ground.material_override = Palette.material(Palette.GROUND)
	ground.position = Vector3(size.x / 2.0, -0.02, size.y / 2.0)
	add_child(ground)


func _add_rects(group_name: String, rects: Array, height: float, base: float, color: Color) -> void:
	var group := _group(group_name)
	for rect: Dictionary in rects:
		_slab(group, rect, height, base, color)


func _group(group_name: String) -> Node3D:
	var group := Node3D.new()
	group.name = group_name
	add_child(group)
	return group


## A box over `rect` from `base` up `height` metres.
static func _slab(parent: Node3D, rect: Dictionary, height: float, base: float, color: Color) -> MeshInstance3D:
	var w := float(rect.get("w", 0.0))
	var d := float(rect.get("d", 0.0))
	var slab := MeshInstance3D.new()
	var box := BoxMesh.new()
	box.size = Vector3(w, height, d)
	slab.mesh = box
	slab.material_override = Palette.material(color)
	slab.position = Vector3(
		float(rect.get("x", 0.0)) + w / 2.0, base + height / 2.0, float(rect.get("z", 0.0)) + d / 2.0)
	parent.add_child(slab)
	return slab
