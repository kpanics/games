#version 450
// The light on a cosmetic's model in the hands (native/src/cosmetics/render.rs), for viewmodel.frag and sticker.frag:
// the sun, the sky round it as a soft light from above and a mirror for the metal, the light of the place it is in (a
// cave is dark), a little light from the eye so a blade in the shade still reads. A face to the sun its own colour and no
// brighter (as the game's blocks in the sun), one away from it in shade (some quarter of it, less under — as CS lights its hands: their folds read, nothing is
// washed flat): what CS's black gloves are stays black.

layout(set = 0, binding = 0) uniform View {
    mat4 proj;
    vec4 sun;
    vec4 sunColor;
    vec4 sky;
    vec4 horizon;
    vec4 params;
    // the Cosmetics page's studio (none in the game): x its shade lifted (the soft boxes round it), y a cool light from
    // behind along its edges (as CS2's inventory's spot_light_rim), z its brights rolled off as Khronos' PBR Neutral
    // rolls them (a product's colours kept, its glints soft)
    vec4 studio;
} view;

const float PI = 3.14159265;

// What the world round mirrors along R (view space, y up): the sky deeper overhead, light at the horizon, the ground
// darker below — as the chams' wet ink sees it (engine/chams.glsl ch_sky), here from the view's own up.
vec3 env(vec3 R) {
    float y = R.y;
    vec3 c = mix(view.horizon.rgb * 1.05 + 0.04, view.sky.rgb * 0.75, smoothstep(0.0, 0.8, y));
    c += (view.horizon.rgb + 0.12) * 0.45 * exp(-(y * y) / 0.015);
    vec3 ground = mix(view.horizon.rgb * 0.4 + 0.03, view.horizon.rgb * 0.15 + 0.02, smoothstep(-0.04, -0.6, y));
    return mix(c, ground, smoothstep(0.02, -0.08, y));
}

float ggx(float nh, float a) {
    float a2 = a * a;
    float d = nh * nh * (a2 - 1.0) + 1.0;
    return a2 / (PI * d * d);
}

float smith(float nv, float nl, float a) {
    float k = (a + 1.0) * (a + 1.0) / 8.0;
    return (nv / (nv * (1.0 - k) + k)) * (nl / (nl * (1.0 - k) + k));
}

vec3 fresnel(vec3 f0, float c) {
    return f0 + (1.0 - f0) * pow(1.0 - c, 5.0);
}

// The same for the world round, a rough surface's edges not turned to mirrors (a matt sleeve gets no grey haze at its
// rim)
vec3 fresnelRough(vec3 f0, float c, float rough) {
    return f0 + (max(vec3(1.0 - rough), f0) - f0) * pow(1.0 - c, 5.0);
}

// Khronos' PBR Neutral: the colours as they are up to near white, the brights above rolled off into it.
vec3 neutral(vec3 color) {
    const float start = 0.8 - 0.04;
    const float desaturation = 0.15;
    float x = min(color.r, min(color.g, color.b));
    float offset = x < 0.08 ? x - 6.25 * x * x : 0.04;
    color -= offset;
    float peak = max(color.r, max(color.g, color.b));
    if (peak < start) {
        return color;
    }
    const float d = 1.0 - start;
    float newPeak = 1.0 - d * d / (peak + d - start);
    color *= newPeak / peak;
    float g = 1.0 - 1.0 / (desaturation * (peak - newPeak) + 1.0);
    return mix(color, vec3(newPeak), g);
}

// The light of the place: the sky's where the sky reaches, a lamp's where it does not (never quite black).
float placeLight() {
    return max(view.sunColor.w, view.sky.w * 0.85);
}

// A surface lit (view space: N its normal, V toward the eye), exposed, its brights given a soft shoulder (a glint does
// not clip flat): the game's colours, linear.
vec3 shade(vec3 albedo, vec3 N, vec3 V, float rough, float metal, float ao, float cloth) {
    float nv = clamp(dot(N, V), 1e-3, 1.0);
    vec3 f0 = mix(vec3(0.04), albedo, metal);
    float a = rough * rough;

    float skyLight = view.sunColor.w;
    float blockLight = view.sky.w;
    float place = placeLight();
    vec3 lamp = vec3(1.0, 0.82, 0.6) * blockLight * (1.0 - skyLight * 0.6);

    vec3 col = vec3(0.0);
    // the sun (or the moon, dimmer): with the sky's light below it, a face to it about its own colour
    vec3 L = normalize(view.sun.xyz);
    float nl = clamp(dot(N, L), 0.0, 1.0);
    float sunPower = view.sun.w * skyLight * 0.62;
    if (nl > 0.0) {
        vec3 H = normalize(L + V);
        float nh = clamp(dot(N, H), 0.0, 1.0);
        vec3 F = fresnel(f0, clamp(dot(H, V), 0.0, 1.0));
        vec3 spec = ggx(nh, a) * smith(nv, nl, a) * F / (4.0 * nv * nl + 1e-4);
        vec3 diff = (1.0 - F) * (1.0 - metal) * albedo / PI;
        col += (diff + spec) * view.sunColor.rgb * nl * sunPower * PI;
    }
    // a soft key from the eye's upper right (as a viewmodel has in CS): the blade's edge catches it in any light
    vec3 K = normalize(vec3(0.45, 0.6, 0.65));
    float nk = clamp(dot(N, K), 0.0, 1.0);
    {
        vec3 H = normalize(K + V);
        float nh = clamp(dot(N, H), 0.0, 1.0);
        vec3 F = fresnel(f0, clamp(dot(H, V), 0.0, 1.0));
        vec3 spec = ggx(nh, a) * smith(nv, max(nk, 1e-3), a) * F / (4.0 * nv * max(nk, 1e-3) + 1e-4);
        vec3 diff = (1.0 - F) * (1.0 - metal) * albedo / PI;
        col += (diff + spec) * nk * (0.04 + 0.06 * place) * PI * mix(vec3(1.0), lamp + 0.3, 1.0 - skyLight);
    }
    // the sky and the place round: a light from above, dim at the sides and dimmer under (the folds and the creases,
    // the occlusion deepened, in shade), a mirror
    vec3 F = fresnelRough(f0, nv, rough);
    vec3 R = reflect(-V, N);
    vec3 skyTint = mix(vec3(1.0), normalize(view.sky.rgb + 0.05) * 1.6, 0.1);
    float up = 0.5 + 0.5 * N.y;
    vec3 envDiff = skyTint * mix(0.07, 0.36, up) * place + lamp * 0.45 * mix(0.4, 1.0, up) + 0.015;
    vec3 envSpec = mix(env(R), env(N), a) * (place * 0.7 + 0.08);
    // the studio's soft boxes: its shade lifted, more from above, what mirrors it lighter
    envDiff += view.studio.x * mix(0.55, 1.0, up);
    envSpec += view.studio.x * 0.5 * (0.6 + 0.4 * smoothstep(-0.3, 0.6, R.y));
    col += (1.0 - F) * (1.0 - metal) * albedo * envDiff * ao * ao;
    col += F * envSpec * mix(ao, 1.0, 0.5) * (1.0 - a * 0.7);
    // cloth: a soft sheen at its edges (CS2's characters' cloth shading, gently)
    col += cloth * albedo * pow(1.0 - nv, 3.0) * 0.35 * (envDiff + view.sunColor.rgb * sunPower * 0.5) * ao;
    // the studio's light from behind (a cool one), only where the surface turns away from the eye
    col += view.studio.y * pow(1.0 - nv, 3.0) * mix(vec3(1.0), albedo, metal * 0.7) * (0.35 + 0.65 * up) * mix(ao, 1.0, 0.6) * vec3(0.92, 0.97, 1.05);
    col *= view.horizon.w;
    if (view.studio.z > 0.5) {
        return neutral(col);
    }
    vec3 over = max(col - 0.8, 0.0);
    return min(col, 0.8) + over / (1.0 + over * 5.0);
}

// Out in the game's colours (sRGB).
vec3 present(vec3 col) {
    return pow(clamp(col, 0.0, 1.0), vec3(1.0 / 2.2));
}
