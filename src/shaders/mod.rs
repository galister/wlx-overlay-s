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

            void main()
            {
                out_color = texture(in_texture, in_uv);
                
                bvec4 cutoff = lessThan(out_color, vec4(0.04045));
                vec4 higher = pow((out_color + vec4(0.055))/vec4(1.055), vec4(2.4));
                vec4 lower = out_color/vec4(12.92);

                out_color = mix(higher, lower, cutoff);
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

                layout (set = 0, binding = 1) uniform ColorBlock {
                    uniform vec4 in_color;
                };


                vec4 supersample(sampler2D tex, vec2 uv) {
                    float ddx = dFdx(uv.x);
                    float ddy = dFdy(uv.y);
                    float width = sqrt(ddx*ddx + ddy*ddy);

                    ivec2 size = textureSize(tex, 0);
                    
                    ivec2 pixelWidth = ivec2(width * vec2(size));
                    ivec2 xy = ivec2(uv * vec2(size));
                    
                    ivec2 start = xy - pixelWidth/2;
                    ivec2 end = xy + pixelWidth/2;
                    
                    vec4 outColor = vec4(0.0);
                    int n = 0;
                    
                    for (int xSample = start.x; xSample <= end.x; xSample++) {
                        for (int ySample = start.y; ySample <= end.y; ySample++) {
                            n++;
                            outColor += texelFetch(tex, clamp(ivec2(xSample, ySample), ivec2(0), size), 0);
                        }
                    }
                    if (n > 0) {
                        return outColor / float(n);
                    } else {
                        return vec4(0.0);
                    }
                }


                float gaussian(float x, float t) {
                    float PI = 3.14159265358;
                    return exp(-x*x/(2.0 * t*t))/(sqrt(2.0*PI)*t);
            }


            float besselI0(float x) {
                return 1.0 + pow(x, 2.0) * (0.25 +  pow(x, 2.0) * (0.015625 +  pow(x, 2.0) * (0.000434028 +  pow(x, 2.0) * (6.78168e-6 +  pow(x, 2.0) * (6.78168e-8 +  pow(x, 2.0) * (4.7095e-10 +  pow(x, 2.0) * (2.40281e-12 + pow(x, 2.0) * (9.38597e-15 + pow(x, 2.0) * (2.8969e-17 + 7.24226e-20 * pow(x, 2.0))))))))));
            }

            float kaiser(float x, float alpha) {
                if (x > 1.0) { 
                    return 0.0;
                }
                return besselI0(alpha * sqrt(1.0-x*x));
            }

            vec4 lowpassFilter(sampler2D tex, vec2 uv, float alpha) {
                float PI = 3.14159265358;
                
                vec4 q = vec4(0.0);
                
                vec2 dx_uv = dFdx(uv);
                vec2 dy_uv = dFdy(uv);
                //float width = sqrt(max(dot(dx_uv, dx_uv), dot(dy_uv, dy_uv)));
                vec2 width = abs(vec2(dx_uv.x, dy_uv.y));
                

                ivec2 size = textureSize(tex, 0);
                
                vec2 pixelWidth = floor(width * vec2(size));
                vec2 aspectRatio = normalize(pixelWidth);
                
                ivec2 xy = ivec2(uv * vec2(size));
                vec2 xyf = uv * vec2(size);
                
                pixelWidth = clamp(pixelWidth, vec2(1.0), vec2(2.0));

                
                ivec2 start = xy - ivec2(pixelWidth);
                ivec2 end = xy + ivec2(pixelWidth);
                
                vec4 outColor = vec4(0.0);
                
                float qSum = 0.0;
                
                for (int v = start.y; v <= end.y; v++) {
                    for (int u = start.x; u <= end.x; u++) {
                        float kx = (xyf.x - float(u))/pixelWidth.x;
                        float ky = (xyf.y - float(v))/pixelWidth.y;
                        
                        //float lanczosValue = gaussian(kx, fcx);
                        float lanczosValue = kaiser(sqrt(kx*kx + ky*ky), alpha);
                        
                        q += texelFetch(tex, ivec2(u, v), 0) * lanczosValue;
                        qSum += lanczosValue;
                    }
                }
                
                return q/qSum;
            }

            void main()
            {
                out_color = lowpassFilter(in_texture, in_uv, 4.0);
                out_color.a = 1.0;
            }
        ",
    }
}
