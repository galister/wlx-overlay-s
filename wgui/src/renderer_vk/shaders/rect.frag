#version 450
#extension GL_GOOGLE_include_directive : enable

precision highp float;

layout(location = 0) in vec4 in_color;
layout(location = 1) in vec4 in_color2;
layout(location = 2) in vec2 in_uv;
layout(location = 3) in vec4 in_border_color;
layout(location = 4) in float in_border_size; // in units
layout(location = 5) in float in_radius;      // in units
layout(location = 6) in vec2 in_rect_size;
layout(location = 0) out vec4 out_color;

#define UNIFORM_PARAMS_SET 0
#include "uniform.glsl"
#include "srgb.glsl"

void main() {
  float rect_aspect = in_rect_size.x / in_rect_size.y;

  vec2 center = in_rect_size / 2.0;
  vec2 coords = in_uv * in_rect_size;

  float radius = in_radius;

  vec2 sdf_rect_dim = center - vec2(radius);
  float sdf = length(max(abs(coords - center), sdf_rect_dim) - sdf_rect_dim) -
              in_radius;

  vec4 color =
      mix(in_color, in_color2, min(length((in_uv - vec2(0.5)) * 2.0), 1.0));

  float pixel_size = 1.0 / uniforms.pixel_scale;

  if (in_border_size < in_radius) {
    // rounded border
    float f = in_border_size > 0.0 ? smoothstep(in_border_size + pixel_size,
                                                in_border_size, -sdf) *
                                         in_border_color.a
                                   : 0.0;
    out_color = mix(color, in_border_color, f);
  } else {
    // square border
    vec2 a = abs(coords - center);
    float aa = center.x - in_border_size;
    float bb = center.y - in_border_size;
    out_color = (a.x > aa || a.y > bb) ? in_border_color : color;
  }

  if (in_radius > 0.0) {
    // rounding cutout alpha
    out_color.a *= 1.0 - smoothstep(-pixel_size, 0.0, sdf);
  }
  out_color.rgb = to_srgb(out_color.rgb);
}
