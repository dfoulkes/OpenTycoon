extends GutTest
## Smoke tests: GUT runs headless, the main scene loads, and the Rust extension is registered.

const Main := preload("res://scripts/main.gd")


func test_gut_runs() -> void:
	assert_eq(1 + 1, 2)


func test_main_scene_loads() -> void:
	var scene: PackedScene = load("res://scenes/main.tscn")
	assert_not_null(scene)
	var main: Node = scene.instantiate()
	add_child_autofree(main)
	assert_eq(main.get_child_count() >= 3, true, "backdrop, menu panels and status label present")


func test_rust_extension_is_registered() -> void:
	assert_true(ClassDB.class_exists("Hello"), "build it with: cargo build -p client-ext")


func test_rust_extension_greets() -> void:
	assert_eq(Main.greeting(), "Hello, London, from OpenTycoon Rust")
