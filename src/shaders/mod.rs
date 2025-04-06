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
        ",
    }
}

//layout (location = 1) in float corner_radius;
//out_color = in_color;
// Some equation that determines whether to keep the pixel
// Use Lerp not if
//out_color.a = 0;

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

pub mod frag_sprite2 {
    vulkano_shaders::shader! {
        ty: "fragment",
        src: r"#version 310 es
            precision highp float;

            layout (location = 0) in vec2 in_uv;
            layout (location = 0) out vec4 out_color;

            layout (set = 0, binding = 0) uniform sampler2D in_texture;
            layout (set = 1, binding = 0) uniform UniBlock {
                uniform vec4 st;
                uniform vec4 mul;
            };

            void main()
            {
                out_color = texture(in_texture, (in_uv * st.xy) + st.zw) * mul;
            }
        ",
    }
}

pub mod frag_sprite2_hl {
    vulkano_shaders::shader! {
        ty: "fragment",
        src: r"#version 310 es
            precision highp float;

            layout (location = 0) in vec2 in_uv;
            layout (location = 0) out vec4 out_color;

            layout (set = 0, binding = 0) uniform sampler2D in_texture;
            layout (set = 1, binding = 0) uniform UniBlock {
                uniform vec4 st;
                uniform vec4 mul;
            };

            void main()
            {
                out_color = texture(in_texture, (in_uv * st.xy) + st.zw).a * mul;
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
                out_color = texture(in_texture, in_uv);
            }
        ",
    }
}

pub mod frag_grid {
    vulkano_shaders::shader! {
        ty: "fragment",
        src: r"#version 310 es
            precision highp float;

            layout (location = 0) in vec2 in_uv;
            layout (location = 0) out vec4 out_color;

            void main()
            {
                float fade = max(1.0 - 2.0 * length(in_uv.xy + vec2(-0.5, -0.5)), 0.0);
                float grid;

                if (fract(in_uv.x / 0.0005) < 0.01 || fract(in_uv.y / 0.0005) < 0.01) {
                    grid = 1.0;
                } else {
                    grid = 0.0;
                }
                out_color = vec4(1.0, 1.0, 1.0, grid * fade);
            }
        ",
    }
}

pub mod frag_screen {
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
                out_color.a = alpha;
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
                out_color.a *= alpha;
            }
        ",
    }
}

pub mod frag_swapchain {
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
                out_color.a *= alpha;
            }
        ",
    }
}

pub mod frag_line {
    vulkano_shaders::shader! {
        ty: "fragment",
        src: r"#version 310 es
            precision highp float;

            layout (location = 0) in vec2 in_uv;
            layout (location = 0) out vec4 out_color;

            layout (set = 0, binding = 0) uniform ColorBlock {
                uniform vec4 in_color;
                uniform vec2 unused;
            };

            void main()
            {
                out_color = in_color;
            }
        ",
    }
}
