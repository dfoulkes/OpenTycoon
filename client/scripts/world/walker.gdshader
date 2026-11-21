// Walkers: each instance's transform holds where it was, INSTANCE_CUSTOM.xy how far it moves
// (x, z) by the next update. The CPU writes both once per delta; the GPU lerps every frame.
shader_type spatial;
render_mode cull_back;

uniform float lerp_t : hint_range(0.0, 1.0) = 1.0;
uniform vec4 albedo : source_color = vec4(0.93, 0.88, 0.78, 1.0);

void vertex() {
	VERTEX.xz += INSTANCE_CUSTOM.xy * lerp_t;
}

void fragment() {
	ALBEDO = albedo.rgb;
	ROUGHNESS = 0.9;
}
