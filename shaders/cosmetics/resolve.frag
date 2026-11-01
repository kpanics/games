#version 450
// The Cosmetics page's look drawn larger (native/src/cosmetics/render.rs frame_large: `ss` times across and down, over
// nothing) brought down into its part of the menu's picture: each pixel the mean of its own ss × ss — their light
// averaged, not their sRGB — laid over the picture as much as they cover it (they come premultiplied by it).

layout(set = 0, binding = 0) uniform sampler2D large;

layout(push_constant) uniform Part {
    ivec2 origin;
    int ss;
    int spare;
} part;

layout(location = 0) out vec4 outColor;

void main() {
    ivec2 base = (ivec2(gl_FragCoord.xy) - part.origin) * part.ss;
    ivec2 last = textureSize(large, 0) - 1;
    vec4 sum = vec4(0.0);
    for (int y = 0; y < part.ss; y++) {
        for (int x = 0; x < part.ss; x++) {
            vec4 t = texelFetch(large, min(base + ivec2(x, y), last), 0);
            vec3 c = t.a > 1e-4 ? t.rgb / t.a : vec3(0.0);
            sum += vec4(pow(clamp(c, 0.0, 1.0), vec3(2.2)) * t.a, t.a);
        }
    }
    sum /= float(part.ss * part.ss);
    vec3 c = sum.a > 1e-4 ? pow(sum.rgb / sum.a, vec3(1.0 / 2.2)) : vec3(0.0);
    outColor = vec4(c * sum.a, sum.a);
}
