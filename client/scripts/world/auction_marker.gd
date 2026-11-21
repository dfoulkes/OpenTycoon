extends Node3D
## The live auction (§10d #6): a bobbing pin over the lot with the current high bid.

const Palette := preload("res://scripts/world/palette.gd")
## Metres above the top of the lot the pin floats.
const HOVER := 8.0
const BOB_METRES := 1.2
const BOB_SPEED := 2.5

var _pin: MeshInstance3D
var _label: Label3D
var _base_y := 0.0
var _clock := 0.0


func _init() -> void:
	visible = false
	_pin = MeshInstance3D.new()
	_pin.name = "Pin"
	var cone := CylinderMesh.new()
	cone.top_radius = 3.0
	cone.bottom_radius = 0.0
	cone.height = 6.0
	_pin.mesh = cone
	_pin.material_override = Palette.marker_material(Palette.AUCTION)
	_pin.position.y = 3.0
	_pin.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	add_child(_pin)
	_label = Label3D.new()
	_label.name = "Bid"
	_label.billboard = BaseMaterial3D.BILLBOARD_ENABLED
	_label.no_depth_test = true
	_label.fixed_size = true
	_label.pixel_size = 0.0015
	_label.font_size = 36
	_label.outline_size = 10
	_label.modulate = Color.WHITE
	_label.outline_modulate = Color(0.1, 0.08, 0.02)
	_label.position.y = 9.0
	add_child(_label)


func _process(delta: float) -> void:
	if not visible:
		return
	_clock += delta
	position.y = _base_y + sin(_clock * BOB_SPEED) * BOB_METRES


## Shows `auction` (the protocol shape) over a lot whose centre is `at` and top is `top_y`.
func show_auction(auction: Dictionary, at: Vector3, top_y: float) -> void:
	_base_y = top_y + HOVER
	position = Vector3(at.x, _base_y, at.z)
	_label.text = label_for(auction)
	visible = true


## Moves the marker without changing its label.
func reposition(at: Vector3, top_y: float) -> void:
	_base_y = top_y + HOVER
	position = Vector3(at.x, _base_y, at.z)


func hide_auction() -> void:
	visible = false


func label_text() -> String:
	return _label.text


## "£1,234 · p1": the high bid in pounds and who holds it.
static func label_for(auction: Dictionary) -> String:
	var bid := format_pounds(int(auction.get("high_bid", 0)))
	var bidder: Variant = auction.get("high_bidder")
	var item := str(auction.get("item", "land")).capitalize()
	if bidder == null:
		return "%s auction\nopening at %s" % [item, bid]
	return "%s auction\n%s · %s" % [item, bid, str(bidder)]


## Integer pence as whole pounds with thousands separators: 123456 -> "£1,234".
static func format_pounds(pence: int) -> String:
	@warning_ignore("integer_division")
	var pounds := absi(pence) / 100
	var digits := str(pounds)
	var grouped := ""
	while digits.length() > 3:
		grouped = "," + digits.right(3) + grouped
		digits = digits.left(digits.length() - 3)
	return ("-£" if pence < 0 else "£") + digits + grouped
