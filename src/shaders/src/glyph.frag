#version 310 es
precision highp float;

layout (location = 0) in vec2 in_uv;
layout (location = 0) out vec4 out_color;

layout (set = 0, binding = 0) uniform sampler2D in_texture;

layout (set = 0, binding = 1) uniform ColorBlock {
    uniform vec4 in_color;
};

void main()
{
    float r = texture(in_texture, in_uv).r;
    out_color = vec4(r,r,r,r) * in_color;
}

