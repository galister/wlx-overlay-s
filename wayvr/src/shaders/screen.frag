#version 310 es
precision highp float;

layout (location = 0) in vec2 in_uv;
layout (location = 0) out vec4 out_color;

layout (set = 0, binding = 0) uniform sampler2D in_texture;

void main()
{
    out_color = texture(in_texture, in_uv);
    out_color.a = 1.0;
}

