#version 310 es
precision highp float;

layout (location = 0) in vec2 in_uv;
layout (location = 0) out vec4 out_color;

layout (set = 0, binding = 0) uniform ColorBlock {
    uniform vec4 in_color;
};

void main()
{
    out_color = in_color;
}

