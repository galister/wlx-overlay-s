#version 310 es
#extension GL_GOOGLE_include_directive : enable

precision highp float;

layout(location = 0) in vec4 in_color;
layout(location = 1) in vec2 in_uv;
layout(location = 2) flat in uint in_content_type;

layout(location = 0) out vec4 out_color;

layout(set = 0, binding = 0) uniform sampler2D color_atlas;
layout(set = 1, binding = 0) uniform sampler2D mask_atlas;

void main() {
  if (in_content_type == 0u) {
    out_color = texture(color_atlas, in_uv) * in_color;
  } else {
    out_color.rgb = in_color.rgb;
    out_color.a = in_color.a * texture(mask_atlas, in_uv).r;
  }
}
