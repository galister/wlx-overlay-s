#version 450
#extension GL_GOOGLE_include_directive : enable

precision highp float;

layout(location = 0) in uint in_model_idx;
layout(location = 1) in uint in_rect_dim;
layout(location = 2) in uint in_border_color;
layout(location = 3) in uint round_border;

layout(location = 0) out vec2 out_uv;
layout(location = 1) out vec4 out_border_color;
layout(location = 2) out float out_border_size;
layout(location = 3) out float out_radius;
layout(location = 4) out vec2 out_rect_size;

#define UNIFORM_PARAMS_SET 0
#define MODEL_BUFFER_SET 1

#include "model_buffer.glsl"
#include "uniform.glsl"

void main() {
  uint v = uint(gl_VertexIndex); // 0-3
  uint rect_width = in_rect_dim & 0xffffu;
  uint rect_height = (in_rect_dim & 0xffff0000u) >> 16u;
  vec2 rect_size = vec2(float(rect_width), float(rect_height));
  float rect_aspect = rect_size.x / rect_size.y;

  // 0.0 - 1.0 normalized
  uvec2 corner_pos_u = uvec2(v & 1u, (v >> 1u) & 1u);
  vec2 corner_pos = vec2(corner_pos_u);
  out_uv = corner_pos;

  mat4 model_matrix = model_buffer.models[in_model_idx];

  out_rect_size = rect_size;

  gl_Position = uniforms.projection * model_matrix * vec4(corner_pos, 0.0, 1.0);

  out_border_color =
      vec4(float((in_border_color & 0x00ff0000u) >> 16u) / 255.0,
           float((in_border_color & 0x0000ff00u) >> 8u) / 255.0,
           float(in_border_color & 0x000000ffu) / 255.0,
           float((in_border_color & 0xff000000u) >> 24u) / 255.0);

  float radius = float(round_border & 0xffu);
  out_radius = radius;

  float border_size = float((round_border & 0xff00u) >> 8);
  out_border_size = border_size;
}
