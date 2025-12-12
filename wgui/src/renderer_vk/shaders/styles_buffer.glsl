struct StylesData {
  float radius;

  vec4 color0;
  vec4 color1;
  uint gradient_style;
  vec2 gradient_curve;

  vec4 border_size_tlbr;
  vec4 border_color0;
  vec4 border_color1;
  vec4 border_color2;
  vec4 border_color3;
  uint border_gradient_style;
  vec2 border_gradient_curve;
}

layout(std140, set = STYLES_BUFFER_SET,
       binding = 0) readonly buffer StylesBuffer {
  StylesData styles[];
}
styles_buffer;
