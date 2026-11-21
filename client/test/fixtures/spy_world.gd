extends Node3D
## Test spy with the world scene's API. Records every call in `calls`.

signal lot_clicked(key: String)

var calls: Array = []


func apply_snapshot(data: Dictionary) -> void:
	calls.append(["apply_snapshot", [data]])


func apply_delta(data: Dictionary) -> void:
	calls.append(["apply_delta", [data]])


func highlight_lot(key: String) -> void:
	calls.append(["highlight_lot", [key]])


func clear_highlight() -> void:
	calls.append(["clear_highlight", []])


func called(method: String) -> Array:
	var out: Array = []
	for c: Array in calls:
		if c[0] == method:
			out.append(c[1])
	return out
