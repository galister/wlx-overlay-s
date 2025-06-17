#version 450
#extension GL_GOOGLE_include_directive : enable

precision highp float;

layout(location = 0) in uint in_model_idx;
layout(location = 1) in uint in_rect_dim;
layout(location = 2) in uint in_uv;
layout(location = 3) in uint in_color;
layout(location = 4) in uint in_content_type;
layout(location = 5) in float depth;
layout(location = 7) in float scale;

layout(location = 0) out vec4 out_color;
layout(location = 1) out vec2 out_uv;
layout(location = 2) flat out uint out_content_type;

layout(set = 0, binding = 0) uniform sampler2D color_atlas;
layout(set = 1, binding = 0) uniform sampler2D mask_atlas;

#define UNIFORM_PARAMS_SET 2
#define MODEL_BUFFER_SET 3

#include "model_buffer.glsl"
#include "uniform.glsl"

void main() {
  uint v = uint(gl_VertexIndex); // 0-3
  uint rect_width = in_rect_dim & 0xffffu;
  uint rect_height = (in_rect_dim & 0xffff0000u) >> 16u;
  vec2 rect_size = vec2(float(rect_width), float(rect_height));
  float rect_aspect = rect_size.x / rect_size.y;

  uvec2 uv = uvec2(in_uv & 0xffffu, (in_uv & 0xffff0000u) >> 16u);

  uvec2 corner_pos_u = uvec2(v & 1u, (v >> 1u) & 1u);
  vec2 corner_pos = vec2(corner_pos_u);
  uvec2 corner_offset = uvec2(rect_width, rect_height) * corner_pos_u;
  uv = uv + corner_offset;

  mat4 model_matrix = model_buffer.models[in_model_idx];

  gl_Position =
      uniforms.projection * model_matrix * vec4(corner_pos * scale, depth, 1.0);

  out_content_type = in_content_type & 0xffffu;

  out_color = vec4(float((in_color & 0x00ff0000u) >> 16u) / 255.0,
                   float((in_color & 0x0000ff00u) >> 8u) / 255.0,
                   float(in_color & 0x000000ffu) / 255.0,
                   float((in_color & 0xff000000u) >> 24u) / 255.0);

  uvec2 dim = uvec2(0, 0);
  if (in_content_type == 0u) {
    dim = uvec2(textureSize(color_atlas, 0));
  } else {
    dim = uvec2(textureSize(mask_atlas, 0));
  }

  out_uv = vec2(uv) / vec2(dim);
}
