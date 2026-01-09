#version 310 es
precision highp float;

layout (location = 0) in vec2 in_uv;
layout (location = 0) out vec4 out_color;

layout (set = 0, binding = 0) uniform ColorBlock {
    uniform vec4 in_color;
    uniform vec2 corner_radius;
};

void main()
{
    out_color.r = corner_radius.r;
    out_color = in_color;

    vec2 uv_circ = ((1. - corner_radius) - (abs(in_uv + vec2(-0.5)) * 2.))/corner_radius;
    float dist = length(uv_circ);

    out_color.a = mix(out_color.a, 0.,
            float(dist > 1.)
            * float(uv_circ.x < 0.)
            * float(uv_circ.y < 0.));
}
