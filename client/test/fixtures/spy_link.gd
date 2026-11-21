extends Node
## Test spy with SimLink's methods and signals. Records every call in `calls`.

signal snapshot_received(data: Dictionary)
signal delta_received(data: Dictionary)
signal game_over(data: Dictionary)
signal link_error(message: String)
signal disconnected

var calls: Array = []


func start_single_player(config_dir: String, economy_path: String, game_seed: int) -> void:
	calls.append(["start_single_player", [config_dir, economy_path, game_seed]])


func set_speed(speed: int) -> void:
	calls.append(["set_speed", [speed]])


func leave() -> void:
	calls.append(["leave", []])


## The argument lists of every call to `method`, oldest first.
func called(method: String) -> Array:
	var out: Array = []
	for c: Array in calls:
		if c[0] == method:
			out.append(c[1])
	return out
