extends RefCounted
## Colours and shared materials for the world scene: one colour per player, the civic colour for
## the city, and a muted colour per district. Materials are cached so 81 lots share a handful.

## One floor of a building, in metres: one lot unit (R48).
const FLOOR_HEIGHT_M := 4.0

## The city's colour: no player ever gets it.
const CIVIC := Color(0.10, 0.62, 0.62)
## A building on unowned land (the protocol allows it during a sale).
const UNOWNED := Color(0.72, 0.72, 0.70)
const GROUND := Color(0.36, 0.42, 0.30)
const STREET := Color(0.22, 0.22, 0.24)
const RIVER := Color(0.16, 0.34, 0.52)
const BRIDGE := Color(0.62, 0.58, 0.50)
const CLOSED_BLOCK := Color(0.45, 0.45, 0.47)
const HIGHLIGHT := Color(1.0, 0.85, 0.2)
const AUCTION := Color(1.0, 0.75, 0.1)

## Players p0..p5 (R: two to six players). Ids beyond these get a generated hue.
const PLAYERS: Array[Color] = [
	Color(0.86, 0.24, 0.22),
	Color(0.24, 0.44, 0.90),
	Color(0.30, 0.74, 0.32),
	Color(0.96, 0.80, 0.18),
	Color(0.62, 0.34, 0.82),
	Color(0.96, 0.54, 0.18),
]

## Muted ground colours for districts, picked by a stable hash of the district name.
const DISTRICTS: Array[Color] = [
	Color(0.62, 0.55, 0.45), Color(0.52, 0.60, 0.50), Color(0.58, 0.52, 0.60),
	Color(0.50, 0.57, 0.64), Color(0.66, 0.60, 0.48), Color(0.56, 0.64, 0.58),
	Color(0.64, 0.54, 0.54), Color(0.54, 0.54, 0.64), Color(0.60, 0.62, 0.52),
	Color(0.52, 0.62, 0.62), Color(0.66, 0.58, 0.56), Color(0.58, 0.58, 0.58),
]

static var _materials: Dictionary = {}


## The colour that marks an owner: a player id (`p0`...), `city` or `unowned`.
static func owner_color(owner_id: String) -> Color:
	if owner_id == "city":
		return CIVIC
	if not owner_id.begins_with("p") or not owner_id.substr(1).is_valid_int():
		return UNOWNED
	var index := owner_id.substr(1).to_int()
	if index >= 0 and index < PLAYERS.size():
		return PLAYERS[index]
	return Color.from_hsv(fmod(index * 0.618034, 1.0), 0.7, 0.9)


## The ground colour of a district: the same name always gives the same colour.
static func district_color(district: String) -> Color:
	return DISTRICTS[absi(district.hash()) % DISTRICTS.size()]


## A shared opaque material of one colour.
static func material(color: Color) -> StandardMaterial3D:
	var key := "flat:%s" % color.to_html()
	if not _materials.has(key):
		var mat := StandardMaterial3D.new()
		mat.albedo_color = color
		mat.roughness = 0.9
		_materials[key] = mat
	return _materials[key]


## A shared unshaded, glowing material for markers that must read at any zoom.
static func marker_material(color: Color, alpha: float = 1.0) -> StandardMaterial3D:
	var key := "marker:%s:%.2f" % [color.to_html(), alpha]
	if not _materials.has(key):
		var mat := StandardMaterial3D.new()
		mat.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
		mat.albedo_color = Color(color, alpha)
		if alpha < 1.0:
			mat.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
		_materials[key] = mat
	return _materials[key]


## A shared material that tints a texture (the building kit's colour map) towards a colour.
static func tinted_material(texture: Texture2D, color: Color) -> StandardMaterial3D:
	var key := "tint:%s:%s" % [texture.resource_path, color.to_html()]
	if not _materials.has(key):
		var mat := StandardMaterial3D.new()
		mat.albedo_texture = texture
		mat.albedo_color = color.lerp(Color.WHITE, 0.25)
		mat.roughness = 0.8
		_materials[key] = mat
	return _materials[key]
