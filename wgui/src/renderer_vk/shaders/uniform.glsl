
// Viewport
layout(std140, set = UNIFORM_PARAMS_SET, binding = 0) uniform UniformParams {
  uniform uvec2 screen_resolution;
  uniform float pixel_scale;
  uniform mat4 projection;
}
uniforms;