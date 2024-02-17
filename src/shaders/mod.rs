pub mod vert_common {
    vulkano_shaders::shader! {
        ty: "vertex",
        src: r"#version 310 es
            precision highp float;

            layout (location = 0) in vec2 in_pos;
            layout (location = 1) in vec2 in_uv;
            layout (location = 0) out vec2 out_uv;

            void main() {
                out_uv = in_uv;
                gl_Position = vec4(in_pos * 2. - 1., 0., 1.);
            }
        ",
    }
}

pub mod frag_color {
    vulkano_shaders::shader! {
        ty: "fragment",
        src: r"#version 310 es
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
        ",
    }
}

pub mod frag_glyph {
    vulkano_shaders::shader! {
        ty: "fragment",
        src: r"#version 310 es
            precision highp float;

            layout (location = 0) in vec2 in_uv;
            layout (location = 0) out vec4 out_color;

            layout (set = 0, binding = 0) uniform sampler2D in_texture;

            layout (set = 1, binding = 0) uniform ColorBlock {
                uniform vec4 in_color;
            };

            void main()
            {
                float r = texture(in_texture, in_uv).r;
                out_color = vec4(r,r,r,r) * in_color;
            }
        ",
    }
}

pub mod frag_sprite {
    vulkano_shaders::shader! {
        ty: "fragment",
        src: r"#version 310 es
            precision highp float;

            layout (location = 0) in vec2 in_uv;
            layout (location = 0) out vec4 out_color;

            layout (set = 0, binding = 0) uniform sampler2D in_texture;

            void main()
            {
                vec4 c = texture(in_texture, in_uv);
                out_color.rgb = c.rgb;
                out_color.a = min((c.r + c.g + c.b)*100.0, 1.0);
            }
        ",
    }
}

pub mod frag_srgb {
    vulkano_shaders::shader! {
        ty: "fragment",
        src: r"#version 310 es
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
                out_color.a = alpha;
            }
        ",
    }
}
