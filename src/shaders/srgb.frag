#version 310 es
precision highp float;

layout (location = 0) in vec2 in_uv;
layout (location = 0) out vec4 out_color;

layout (set = 0, binding = 0) uniform sampler2D in_texture;
layout (set = 1, binding = 0) uniform AlphaBlock {
    uniform float alpha;
};


void main()
{
    out_color = texture(in_texture, in_uv);
    
    bvec4 cutoff = lessThan(out_color, vec4(0.04045));
    vec4 higher = pow((out_color + vec4(0.055))/vec4(1.055), vec4(2.4));
    vec4 lower = out_color/vec4(12.92);

    out_color = mix(higher, lower, cutoff);
    out_color.a *= alpha;
}

