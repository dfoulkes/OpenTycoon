extends Node3D
## The inspector's 3D world (§10d): the board, lots, walkers, the live auction and the camera.
##
## Fed only through `apply_snapshot` / `apply_delta` with the parsed protocol messages
## (docs/inspector-protocol.md), so it runs from a fixture as well as from the server. Picking a
## lot with `select` emits `lot_clicked`; the lot card and HUD live in other scenes.
##
## Public API: `apply_snapshot(data)`, `apply_delta(data)`, `signal lot_clicked(key)`,
## `highlight_lot(key)`, `clear_highlight()`, `camera_rig()`. The rest is read-only state for
## tests and other scenes.

signal lot_clicked(key: String)

const LotView := preload("res://scripts/world/lot_view.gd")
const CameraRig := preload("res://scripts/world/camera_rig.gd")
const RAY_LENGTH := 4000.0

var _lots: Dictionary = {}
var _auction_block := ""

@onready var _board: Node3D = $Board
@onready var _lot_root: Node3D = $Lots
@onready var _walkers: MultiMeshInstance3D = $Walkers
@onready var _auction: Node3D = $AuctionMarker
@onready var _highlight: Node3D = $Highlight
@onready var _rig: CameraRig = $CameraRig


## Builds the board and every lot from a `snapshot` message.
func apply_snapshot(data: Dictionary) -> void:
	var board: Dictionary = data.get("board", {})
	_board.build(board)
	_rebuild_lots(board.get("blocks", []))
	var size := Vector2(float(board.get("width_m", 0.0)), float(board.get("depth_m", 0.0)))
	_rig.set_bounds(Rect2(Vector2.ZERO, size))
	_rig.focus_on(size / 2.0)
	clear_highlight()
	_auction_block = ""
	_auction.hide_auction()
	apply_delta(data)


## Applies a `delta` (or the dynamic half of a snapshot): changed lots, the auction, walkers.
func apply_delta(data: Dictionary) -> void:
	for lot: Dictionary in data.get("lots", []):
		var view: LotView = _lots.get(str(lot.get("key", "")))
		if view != null:
			view.apply(lot)
	if data.has("auction"):
		_show_auction(data["auction"])
	elif _auction_block != "":
		_show_auction_over(_auction_block)
	if data.has("walkers"):
		_walkers.set_positions(data["walkers"])


## Frames lot `key`; an unknown key clears the highlight.
func highlight_lot(key: String) -> void:
	var view: LotView = _lots.get(key)
	if view == null:
		clear_highlight()
		return
	_highlight.frame(key, view.position, view.footprint())


func clear_highlight() -> void:
	_highlight.clear()


func camera_rig() -> CameraRig:
	return _rig


## The lot under a screen point of the world's camera, or "".
func lot_key_at_screen(screen: Vector2) -> String:
	var cam := _rig.camera()
	var from := cam.project_ray_origin(screen)
	return lot_key_at(from, from + cam.project_ray_normal(screen) * RAY_LENGTH)


## The lot the segment `from` → `to` hits first, or "".
func lot_key_at(from: Vector3, to: Vector3) -> String:
	var query := PhysicsRayQueryParameters3D.create(from, to, LotView.PICK_LAYER)
	var hit := get_world_3d().direct_space_state.intersect_ray(query)
	if hit.is_empty():
		return ""
	var collider := hit["collider"] as Object
	return str(collider.get_meta("lot_key", "")) if collider != null else ""


## Emits `lot_clicked` for the lot the segment hits, if any.
func select_at(from: Vector3, to: Vector3) -> void:
	var key := lot_key_at(from, to)
	if key != "":
		lot_clicked.emit(key)


func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventMouseButton and event.is_action_pressed("select"):
		var screen := (event as InputEventMouseButton).position
		var key := lot_key_at_screen(screen)
		if key != "":
			lot_clicked.emit(key)
			get_viewport().set_input_as_handled()


# --- read-only state, for tests and other scenes ---

func block_count() -> int:
	return _board.block_count()


func block_node(key: String) -> Node3D:
	return _board.block(key)


func block_color(key: String) -> Color:
	var block: MeshInstance3D = _board.block(key)
	return (block.material_override as StandardMaterial3D).albedo_color if block != null else Color.BLACK


func feature_count(kind: String) -> int:
	return _board.feature_count(kind)


func lot_view(key: String) -> LotView:
	return _lots.get(key)


func walker_count() -> int:
	return _walkers.count()


func auction_marker_visible() -> bool:
	return _auction.visible


func auction_label() -> String:
	return _auction.label_text()


func auction_marker_position() -> Vector3:
	return _auction.position


func highlighted_lot() -> String:
	return _highlight.lot_key


func highlight_visible() -> bool:
	return _highlight.visible


func _rebuild_lots(blocks: Array) -> void:
	for view: Node in _lot_root.get_children():
		_lot_root.remove_child(view)
		view.free()
	_lots.clear()
	for block: Dictionary in blocks:
		var view := LotView.new()
		_lot_root.add_child(view)
		view.setup(block)
		_lots[view.key] = view


func _show_auction(auction: Variant) -> void:
	if auction is Dictionary:
		_auction_block = str((auction as Dictionary).get("block", ""))
		var view: LotView = _lots.get(_auction_block)
		if view != null:
			_auction.show_auction(auction, view.position, view.top())
			return
	_auction_block = ""
	_auction.hide_auction()


## Keeps the marker above its lot when the lot's building changes height.
func _show_auction_over(key: String) -> void:
	var view: LotView = _lots.get(key)
	if view != null:
		_auction.reposition(view.position, view.top())
