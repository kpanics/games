#version 450
// A glove paint worked in the gloves' texture space (native/src/cosmetics/render.rs runs CS2's own glove compositor
// over it): CS2's vertex shader for it (csgo_customglove vs, its backwards-compatible combo) as ours — one triangle
// over the target, each point its texture coordinates and the four layers', the pattern's (mirrored for the right
// hand), the damage's and the grime's, as CS2's own works them out of its constants (native/src/cosmetics/glove.rs
// lays them at CS2's byte offsets: read here a float at a time).

layout(set = 1, binding = 1) uniform Vs {
    vec4 w[15];
} c;

layout(location = 0) out vec4 vUv;
layout(location = 1) out vec4 vPattern;
layout(location = 2) out vec4 vLayers12;
layout(location = 3) out vec4 vLayers34;
layout(location = 4) out vec4 vDamageGrime;
layout(location = 5) out vec3 vNormal;
layout(location = 6) out vec4 vTangent;

float f(int at) {
    int i = at / 4;
    return c.w[i / 4][i % 4];
}

vec2 f2(int at) {
    return vec2(f(at), f(at + 4));
}

vec4 f4(int at) {
    return vec4(f(at), f(at + 4), f(at + 8), f(at + 12));
}

void main() {
    vec2 p = vec2(float((gl_VertexIndex << 1) & 2), float(gl_VertexIndex & 2));
    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
    vec2 uv = p;
    // g_bFlipFixup (196): the pattern's U turned round
    vec2 fu = floatBitsToInt(f(196)) != 0 ? vec2(-uv.x, uv.y) : uv;
    vec4 x0 = f4(204);
    vec4 x1 = f4(220);
    vUv = uv.xyxy;
    // the pattern's place; g_fPatternNormalDistortion (200) times g_vPatternTexCoordScale (176)
    vPattern = vec4(dot(fu, x0.xy) + x0.w, dot(fu, x1.xy) + x1.w, vec2(f(200)) * f2(176));
    // each layer's scale and offset (g_vUvScale1 16, g_vUvOffset1 24 …)
    vLayers12 = vec4(uv * f2(16) + f2(24), uv * f2(32) + f2(40));
    vLayers34 = vec4(uv * f2(48) + f2(56), uv * f2(64) + f2(72));
    // the damage's and the grime's transforms (two rows each: 104, 120, 136, 152)
    vec4 d0 = f4(104);
    vec4 d1 = f4(120);
    vec4 g0 = f4(136);
    vec4 g1 = f4(152);
    vDamageGrime = vec4(dot(uv, d0.xy) + d0.w, dot(uv, d1.xy) + d1.w, dot(uv, g0.xy) + g0.w, dot(uv, g1.xy) + g1.w);
    vNormal = vec3(0.0, 0.0, 1.0);
    vTangent = vec4(1.0, 0.0, 0.0, 1.0);
}
