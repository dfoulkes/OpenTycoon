extends Node3D
## Grey-box placeholder scene: a 3×3 grid of lots and a camera. Real city rendering lands in M3.

const LOT_METRES := 36.0  # 9 lot units × 4 m (R48)
const GAP_METRES := 8.0   # road between blocks


func _ready() -> void:
	for row in 3:
		for col in 3:
			var lot := MeshInstance3D.new()
			var mesh := BoxMesh.new()
			mesh.size = Vector3(LOT_METRES, 0.5, LOT_METRES)
			lot.mesh = mesh
			lot.position = Vector3(col * (LOT_METRES + GAP_METRES), 0, row * (LOT_METRES + GAP_METRES))
			add_child(lot)
	$Status.text = greeting()


## Greets through the Rust extension if it loaded, so a missing build is obvious on screen.
static func greeting() -> String:
	if not ClassDB.class_exists("Hello"):
		return "Rust extension not loaded: run `cargo build -p client-ext`"
	var hello: Object = ClassDB.instantiate("Hello")
	var text: String = hello.call("greet", "London")
	hello.free()
	return text
