#version 310 es
precision highp float;

layout (location = 0) in vec2 in_pos;
layout (location = 1) in vec2 in_uv;
layout (location = 0) out vec2 out_uv;

void main() {
    out_uv = in_uv;
    gl_Position = vec4(in_pos * 2. - 1., 0., 1.);
}
