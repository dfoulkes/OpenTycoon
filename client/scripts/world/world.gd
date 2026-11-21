extends Node3D
## The world scene (stub: implementation follows the tests).

signal lot_clicked(key: String)


func apply_snapshot(_data: Dictionary) -> void:
	pass


func apply_delta(_data: Dictionary) -> void:
	pass


func highlight_lot(_key: String) -> void:
	pass


func clear_highlight() -> void:
	pass


func camera_rig() -> Node3D:
	return $CameraRig


func block_count() -> int:
	return 0


func block_node(_key: String) -> Node3D:
	return null


func block_color(_key: String) -> Color:
	return Color.BLACK


func feature_count(_kind: String) -> int:
	return 0


func lot_view(_key: String) -> Object:
	return null


func walker_count() -> int:
	return 0


func auction_marker_visible() -> bool:
	return false


func auction_label() -> String:
	return ""


func auction_marker_position() -> Vector3:
	return Vector3.ZERO


func highlighted_lot() -> String:
	return ""


func highlight_visible() -> bool:
	return false


func lot_key_at(_from: Vector3, _to: Vector3) -> String:
	return ""


func select_at(_from: Vector3, _to: Vector3) -> void:
	pass
