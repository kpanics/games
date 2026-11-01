// A sticker over a model's body (native/src/cosmetics/render.rs; light.glsl before it): CS2's own sticker shader's
// surface (csgo_weapon_sticker, its pixel shader read back) — the picture's alpha sharpened, its paper's normal and
// roughness, foil's metal tinted, glitter's sparkle, the holographic spectrum by the angle it is seen at, wear eating
// it from its scratches (a paper one's backing showing first) — lit as the body is, laid over it.

layout(location = 0) in vec3 vPos;
layout(location = 1) in vec3 vNormal;
layout(location = 2) in vec4 vTangent;
layout(location = 3) in vec2 vUv;
layout(location = 4) in vec4 vSticker;
layout(location = 5) in vec3 vAcross;
layout(location = 6) in vec3 vUp;

layout(set = 0, binding = 2) uniform sampler2D stickerTex;   // g_tSticker0 (sRGB)
layout(set = 0, binding = 3) uniform sampler2D roughTex;     // g_tNormalRoughnessSticker0: normal (hemi-octahedron) rg, roughness b, self-lit a
layout(set = 0, binding = 4) uniform sampler2D sfxTex;       // g_tSfxMaskSticker0: holographic amount r, the spectrum's offset g and row b; glitter's mask a
layout(set = 0, binding = 5) uniform sampler2D spectrumTex;  // g_tHoloSpectrumSticker0 (sRGB)
layout(set = 0, binding = 6) uniform sampler2D glitterTex;   // g_tGlitterNormalSticker0
layout(set = 0, binding = 7) uniform sampler2D scratchTex;   // g_tStickerScratches
layout(set = 0, binding = 8) uniform sampler2D spectrumClamp; // the spectrum, its rows clamped (g_bClampSpectrumVSticker0)

// the material's (after the vertex shader's projection): its switches (bit 0 holographic, 1 metallic, 2 glitter, 3 paper
// backing, 4 automatic PBR colour fitting, 5 legacy tint multiply, 6 self-lit, 7 the spectrum's rows clamped, 8 its
// roughness kept when worn), its wear, its colour boost, its effects' boost; its tint (w how much it greys first); its
// wear's bias (xy), its scratches' (z), glitter's scale (w)
layout(push_constant) uniform Sticker {
    layout(offset = 64) uint flags;
    float wear;
    float colorBoost;
    float sfxBoost;
    vec4 tint;
    vec4 wearing;
    // how much of it is seen yet (coming in once its picture is there)
    float seen;
} st;

layout(location = 0) out vec4 outColor;

const vec3 LUM = vec3(0.2125, 0.7154, 0.0721);
// CS2's derived levels (g_flReflectance's expressions, its defaults): the lowest albedo, its curve, its highest
const vec3 ALBEDO_LEVELS = vec3(0.045, 0.7369681, 0.85);
const vec3 DARK_METAL_LEVELS = vec3(0.1, 0.7369681, 0.92);
const vec3 METAL_LEVELS = vec3(0.45, 0.7369681, 0.92);
const vec3 HOLO_LEVELS = vec3(0.05, 0.6214905, 0.85);
const float COLOR_BOOST_FACTOR = 63.0;

bool on(uint bit) {
    return (st.flags & (1u << bit)) != 0u;
}

// CS2's hemi-octahedral normal (x + y, x - y)
vec3 hemiOct(vec2 t) {
    float a = t.x + t.y - 1.0039216;
    float b = t.x - t.y;
    return normalize(vec3(a, b, 1.0 - abs(a) - abs(b)));
}

float max3(vec3 v) {
    return max(v.x, max(v.y, v.z));
}

// a colour fitted to what a real surface can be (CS2's automatic PBR colour fitting): its hue kept, its brightness
// between the levels' lowest and highest along their curve
vec3 fitted(vec3 nc, float L, vec3 levels, float lo, float boost, float peak) {
    vec3 f1 = max((nc * lo * 1.732) / length(nc) / dot(nc, LUM), nc * mix(levels.x, levels.z, clamp(pow(peak * boost, levels.y), 0.0, 1.0)));
    return mix(vec3(lo), mix(f1, min(vec3(levels.z), f1 + L), max(0.0, boost - 1.0) * 0.015873017), pow(smoothstep(3e-4, lo, L), 0.5));
}

// glitter's sparkle off one layer of its flakes (n their normal in the view)
vec3 sparkle(vec3 V, vec3 n) {
    vec3 s = sin(reflect(V, n) * 12.0);
    vec3 k = max(vec3(0.0), s - 0.99) * 100.0001;
    return vec3(pow(dot(clamp(-s, 0.0, 1.0), LUM), 4.0)) + vec3(dot(clamp(vec3(0.15) - s, 0.0, 1.0), LUM) * 0.25) + (k + pow(k.yzx, vec3(4.0))) * 3.0;
}

void main() {
    vec2 uv = vSticker.xy;
    // off the sticker, too far out of its plane, or on a face turned away from it: nothing
    if (any(lessThan(uv, vec2(0.0))) || any(greaterThan(uv, vec2(1.0))) || abs(vSticker.z) > 1.0 || vSticker.w < 0.15) {
        discard;
    }
    vec3 V = normalize(-vPos);
    vec3 N0 = normalize(vNormal);
    if (dot(N0, V) < 0.0) {
        N0 = -N0;
    }
    // the sticker's own frame on the surface: across, up (its picture runs down), out
    vec3 T = normalize(vAcross - N0 * dot(N0, vAcross));
    vec3 B = normalize(vUp - N0 * dot(N0, vUp));
    float wear = clamp(st.wear, 0.0, 1.0);

    vec4 art = texture(stickerTex, uv, -1.0);
    float alpha = clamp(art.a * 12.75, 0.0, 1.0);
    // its paper's edge: a little of it seen past the picture's edge, toward the eye (when not too small to tell)
    float lod = textureQueryLod(stickerTex, uv).x;
    float near = 1.0 - clamp(lod - 3.0, 0.0, 1.0);
    if (near > 0.0) {
        vec2 off = vec2(dot(V, T), -dot(V, B)) * 0.01;
        float edge = clamp(texture(stickerTex, clamp(uv - off, 0.0, 1.0), 1.0).a * 12.75, 0.0, 1.0) * 0.7;
        alpha = mix(alpha, max(alpha, edge), near);
    }
    float soft = near > 0.0 ? clamp(art.a * 12.75, 0.0, 1.0) : 1.0;
    vec4 nr = texture(roughTex, uv, -1.0);
    vec3 ns = hemiOct(nr.xy);
    ns.y = -ns.y;
    vec3 tn = normalize(vec3(0.0, 0.0, 1.0) + ns * alpha * 2.0);
    float ao = min(1.0, soft * 0.5 + 0.5);
    float rough = nr.z;
    float metal = on(1u) ? 1.0 : 0.0;

    vec3 glitter = vec3(0.0);
    if (on(2u)) {
        vec4 mask = texture(sfxTex, uv, -1.0);
        float g = 1.0 - mask.a;
        if (g != 0.0) {
            float scale = 2.5 * st.wearing.w;
            vec4 guv = vec4(uv * scale, (vec2(0.5) + uv) * scale);
            vec2 dd = max(dFdx(guv.xy), dFdy(guv.xy));
            vec4 g1 = texture(glitterTex, guv.xy);
            vec3 n1 = hemiOct(g1.xy);
            vec3 w1 = n1 * n1.z;
            vec3 c1 = sparkle(V, normalize(T * w1.x + B * w1.y + N0 * w1.z));
            float amount = 0.04 * g * clamp(1.0 - min(dd.x, dd.y) * 40.0, 0.0, 1.0);
            tn.xy += w1.xy * amount;
            vec4 g2 = textureLod(glitterTex, guv.zw, 0.0);
            vec3 n2 = hemiOct(g2.xy);
            vec3 c2 = sparkle(V, normalize(T * n2.x + B * n2.y + N0 * n2.z));
            vec3 base = art.rgb;
            if (on(4u)) {
                vec3 nc = normalize(max(vec3(3e-4), art.rgb)) * 1.06;
                base = mix(vec3(ALBEDO_LEVELS.x), max((nc * ALBEDO_LEVELS.x * 1.732) / length(nc) / dot(nc, LUM),
                    nc * mix(ALBEDO_LEVELS.x, ALBEDO_LEVELS.z, clamp(pow(max3(art.rgb), ALBEDO_LEVELS.y), 0.0, 1.0))),
                    pow(smoothstep(3e-4, ALBEDO_LEVELS.x, dot(clamp(art.rgb, 0.0, 1.0), LUM)), 0.5));
            }
            tn.xy += n2.xy * amount * g2.w;
            glitter = max(c1 * g1.w, c2 * g2.w) * base * st.sfxBoost * g;
            metal = max(metal, g * 0.5);
        }
    }

    // its colour
    vec3 albedo;
    if (on(4u)) {
        vec3 tint = mix(vec3(1.0), st.tint.rgb, metal);
        float grey = mix(0.0, st.tint.w, metal);
        vec3 c;
        float L;
        vec3 nc;
        if (on(5u)) {
            c = clamp(mix(art.rgb, vec3(dot(art.rgb, LUM)), grey) * tint, 0.0, 1.0);
            nc = normalize(max(vec3(3e-4), c)) * 1.06;
            L = dot(clamp(c * st.colorBoost, 0.0, 1.0), LUM);
        } else {
            c = art.rgb;
            nc = normalize(max(vec3(3e-4), mix(tint * art.rgb, tint, grey))) * 1.06;
            L = dot(clamp(art.rgb * st.colorBoost, 0.0, 1.0), LUM);
        }
        vec3 levels = mix(ALBEDO_LEVELS, mix(METAL_LEVELS, DARK_METAL_LEVELS, smoothstep(0.55, 0.0156, L)), metal);
        float lo = mix(ALBEDO_LEVELS.x, DARK_METAL_LEVELS.x, metal);
        albedo = fitted(nc, L, levels, lo, st.colorBoost, max3(c));
    } else if (on(1u)) {
        albedo = mix(art.rgb, vec3(dot(art.rgb, LUM)), st.tint.w) * st.tint.rgb * (on(5u) ? st.colorBoost : 1.0);
    } else {
        albedo = art.rgb;
    }
    vec3 lit = on(6u) ? albedo * nr.a * 2.0 : vec3(0.0);

    // holographic: the spectrum by the angle it is seen at, where its mask says
    if (on(0u)) {
        vec4 m = texture(sfxTex, uv, -1.0);
        if (m.r > 0.0) {
            vec2 suv = vec2(m.g + dot(V, N0) + dot(V, normalize(view.sun.xyz)), m.b);
            vec3 spec = on(7u) ? texture(spectrumClamp, suv, -1.0).rgb : texture(spectrumTex, suv, -1.0).rgb;
            float L = dot(clamp(spec * st.sfxBoost, 0.0, 1.0), LUM);
            vec3 nc = normalize(max(vec3(3e-4), spec)) * 1.06;
            vec3 f1 = max((nc * HOLO_LEVELS.x * 1.732) / length(nc) / dot(nc, LUM), nc * mix(HOLO_LEVELS.x, HOLO_LEVELS.z, clamp(pow(max3(spec) * st.sfxBoost, HOLO_LEVELS.y), 0.0, 1.0)));
            vec3 holo = mix(vec3(HOLO_LEVELS.x), mix(f1, min(vec3(HOLO_LEVELS.z), f1 + L * st.sfxBoost), max(0.0, st.sfxBoost - 1.0) / COLOR_BOOST_FACTOR), pow(smoothstep(3e-4, HOLO_LEVELS.x, L), 0.5));
            albedo = mix(albedo, holo, m.r);
            metal *= 1.0 - m.r;
        }
    }

    // worn: eaten from its scratches, a paper one's backing showing first; rougher where it is going
    if (wear > 0.0) {
        float scratch = 1.0 - min(st.wearing.z, texture(scratchTex, uv, -1.0).r);
        float s = mix(scratch, scratch * 0.5, wear);
        bool paper = on(3u);
        float bias = paper ? 0.5 : ((st.wearing.y * (st.wearing.y > 0.0 ? 0.5 : 0.25)) + 0.5) * 0.5;
        float e = clamp(wear - pow(clamp((art.a - 0.078431375) * 1.0851064, 0.0, 1.0), st.wearing.x * st.wearing.x), 0.0, 1.0);
        if (paper) {
            float t = clamp(e * 2.0, 0.0, 1.0) + st.wearing.y * wear;
            albedo = mix(vec3(mix(0.7, 0.2 + 0.4 * s, wear)), albedo, smoothstep(t, t + 0.1, s));
        }
        float e2 = clamp(e * (1.0 + bias) - bias, 0.0, 1.0);
        if (!on(1u) && !on(8u)) {
            rough = mix(rough, 0.8, step(s, smoothstep(0.0, 0.35, wear)));
        }
        albedo = mix(albedo, albedo * s, wear * 0.3);
        alpha *= smoothstep(e2, e2 + 0.1, s);
    }
    tn.xy = mix(uv - 0.5, tn.xy, alpha);

    if (alpha <= 0.0) {
        discard;
    }
    vec3 N = normalize(T * tn.x + B * tn.y + N0 * max(tn.z, 0.05));
    vec3 col = shade(albedo, N, V, clamp(rough, 0.04, 1.0), metal, ao, 0.0);
    col += (glitter * (placeLight() * 0.6 + 0.2) + lit) * view.horizon.w;
    outColor = vec4(present(col), clamp(alpha * ao, 0.0, 1.0) * st.seen * (1.0 - view.params.y));
}
