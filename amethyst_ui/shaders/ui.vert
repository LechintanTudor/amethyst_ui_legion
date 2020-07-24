#version 450

const vec2[4] POSITIONS = vec2[](
    vec2(0.5, -0.5),  // Bottom right
    vec2(-0.5, -0.5), // Bottom left
    vec2(0.5, 0.5),   // Top right
    vec2(-0.5, 0.5)   // Top left
);

layout(std140, set = 0, binding = 0) uniform UiViewArgs {
	vec2 inverse_window_half_size;
};

layout(location = 0) in vec2 position;
layout(location = 1) in vec2 dimensions;
layout(location = 2) in vec4 tex_coords_bounds;
layout(location = 3) in vec4 color;
layout(location = 4) in vec4 color_bias;

layout(location = 0) out vec2 out_tex_coords;
layout(location = 1) out vec4 out_color;
layout(location = 2) out vec4 out_color_bias;

void main() {
	vec2 center = vec2(position.x * inverse_window_half_size.x, -position.y * inverse_window_half_size.y);
	vec2 final_position = center + dimensions * POSITIONS[gl_VertexIndex] * inverse_window_half_size;

	gl_Position = vec4(final_position, 0.0, 1.0);
    out_tex_coords = mix(tex_coords_bounds.xy, tex_coords_bounds.zw, POSITIONS[gl_VertexIndex] + vec2(0.5));
    out_color = color;
    out_color_bias = color_bias;
}