vec3 to_srgb(in vec3 in_color) {
    bvec3 cutoff = lessThan(in_color, vec3(0.04045));
    vec3 higher = pow((in_color + vec3(0.055))/vec3(1.055), vec3(2.4));
    vec3 lower = in_color/vec3(12.92);
    return mix(higher, lower, cutoff);
}
