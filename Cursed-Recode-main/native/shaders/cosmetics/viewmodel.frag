// A cosmetic's model in the hands, lit (native/src/cosmetics/render.rs; light.glsl before it): its base colour, its
// normal map, its metalness and roughness (glTF's packing: g roughness, b metalness, r occlusion when there is one).
// Out in the game's colours (sRGB).

layout(location = 0) in vec3 vPos;
layout(location = 1) in vec3 vNormal;
layout(location = 2) in vec4 vTangent;
layout(location = 3) in vec2 vUv;
layout(location = 4) in vec3 vModel;

layout(set = 0, binding = 2) uniform sampler2D baseTex;
layout(set = 0, binding = 3) uniform sampler2D normalTex;
layout(set = 0, binding = 4) uniform sampler2D packedTex;
// the occlusion's own texture (a finish's packed texture has none: the weapon's own then)
layout(set = 0, binding = 5) uniform sampler2D aoTex;

// bit 0: the packed texture is a weapon finish's, as CS2 lays it (roughness r, metalness g); bit 1: a glove finish's
// textures (CS2's glove compositor's): the normal's hemi-octahedron in rg and its roughness in b and a, the metalness
// in g and the cloth in b, the occlusion its own
// bit 2: StatTrak's display (CS2's csgo_weapon F_STATTRAK; bit 3 a knife's, alpha-tested): its texture the digits' atlas (16 by 8: the first row a
// dim leading 0, then 0..9 lit), each of the six places' u its place (whole) and its own cell's column (the rest);
// `value` the number shown, the digits lit by themselves
// bit 4: the item's reflection in the page's floor (view.params.z the floor's height, w how far under it the reflection
// fades): nothing of it over the floor, its textures a little soft, lit as the item is where the floor sees it, faint
// `look` our material over the finish (0 none, 1 the shadow, 2 the polymorph's liquid metal, 6 its own colour, 7 the
// ink, 8 the curse — the Chams' looks; cosmetics/overlay.rs), how much of it, its colour (linear)
layout(push_constant) uniform Material {
    uint flags;
    uint value;
    uint look;
    float strength;
    vec4 colour;
} material;

// the place's digit's column: a dim 0 before the number's first digit, else the digit's (1 + d)
float digitColumn(float place) {
    uint p = uint(6.0 - place);
    uint scale = 1u;
    for (uint i = 0u; i < p; i++) {
        scale *= 10u;
    }
    uint v = min(material.value, 999999u);
    if (v < scale && p > 0u) {
        return 0.0;
    }
    return float((v / scale) % 10u) + 1.0;
}

layout(location = 0) out vec4 outColor;

const vec3 LUMA = vec3(0.2126, 0.7152, 0.0722);

float hash(vec3 p) {
    p = fract(p * 0.3183099 + 0.1);
    p *= 17.0;
    return fract(p.x * p.y * p.z * (p.x + p.y + p.z));
}

float noise(vec3 p) {
    vec3 i = floor(p);
    vec3 f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    return mix(mix(mix(hash(i), hash(i + vec3(1, 0, 0)), f.x), mix(hash(i + vec3(0, 1, 0)), hash(i + vec3(1, 1, 0)), f.x), f.y),
               mix(mix(hash(i + vec3(0, 0, 1)), hash(i + vec3(1, 0, 1)), f.x), mix(hash(i + vec3(0, 1, 1)), hash(i + vec3(1, 1, 1)), f.x), f.y), f.z);
}

float fbm(vec3 p) {
    float v = 0.0;
    float a = 0.5;
    for (int i = 0; i < 4; i++) {
        v += a * noise(p);
        p = p * 2.03 + vec3(1.7, 9.2, 3.1);
        a *= 0.5;
    }
    return v;
}

// the cells' edges (Worley): the distance from a point to the border between its two nearest cells, and the
// nearest's own number
vec2 cells(vec3 p) {
    vec3 i = floor(p);
    vec3 f = fract(p);
    float d1 = 8.0;
    float d2 = 8.0;
    float id = 0.0;
    for (int z = -1; z <= 1; z++) {
        for (int y = -1; y <= 1; y++) {
            for (int x = -1; x <= 1; x++) {
                vec3 g = vec3(x, y, z);
                vec3 c = g + vec3(hash(i + g), hash(i + g + 19.1), hash(i + g + 47.3)) - f;
                float d = dot(c, c);
                if (d < d1) {
                    d2 = d1;
                    d1 = d;
                    id = hash(i + g + 7.7);
                } else if (d < d2) {
                    d2 = d;
                }
            }
        }
    }
    return vec2(sqrt(d2) - sqrt(d1), id);
}

void main() {
    bool mirror = (material.flags & 16u) != 0u;
    float under = view.params.z - vPos.y;
    if (mirror && under < 0.0) {
        discard;
    }
    float soft = mirror ? 1.5 : 0.0;
    bool stattrak = (material.flags & 4u) != 0u;
    vec4 base;
    float illum = 0.0;
    if (stattrak) {
        float place = clamp(floor(vUv.x), 1.0, 6.0);
        float within = fract(vUv.x) - (place - 1.0) / 16.0;
        vec2 auv = vec2(digitColumn(place) / 16.0 + within, fract(vUv.y));
        base = textureGrad(baseTex, auv, dFdx(vUv), dFdy(vUv));
        // bit 3: a knife's (its digits scratched into the blade: the atlas's alpha, tested; no light of their own)
        if ((material.flags & 8u) != 0u) {
            if (base.a < 0.667) {
                discard;
            }
        } else {
            illum = 1.2;
        }
    } else {
        base = texture(baseTex, vUv, soft);
    }
    vec3 packed = texture(packedTex, vUv, soft).rgb;
    bool cs2 = (material.flags & 1u) != 0u;
    bool glove = (material.flags & 2u) != 0u;
    vec4 nt = texture(normalTex, vUv, soft);
    float rough = clamp(glove ? (nt.b + nt.a) * 0.5 : cs2 ? packed.r : packed.g, 0.04, 1.0);
    float metal = glove || cs2 ? packed.g : packed.b;
    float cloth = glove ? packed.b : 0.0;
    float ao = glove || cs2 ? texture(aoTex, vUv, soft).r : packed.r;
    vec3 tn = nt.xyz * 2.0 - 1.0;
    if (glove) {
        // CS2's hemi-octahedron (as its compositor reads its own: x + y, x - y)
        float hx = nt.x + nt.y - 1.0039216;
        float hy = nt.x - nt.y;
        tn = vec3(hx, hy, 1.0 - abs(hx) - abs(hy));
    }
    vec3 N0 = normalize(vNormal);
    vec3 T = normalize(vTangent.xyz - N0 * dot(N0, vTangent.xyz));
    vec3 B = cross(N0, T) * vTangent.w;
    vec3 N = normalize(T * tn.x + B * tn.y + N0 * tn.z);
    vec3 V = normalize(-vPos);
    if (mirror) {
        // the item's own surface, and the way from it to the floor's mirror
        N.y = -N.y;
        V.y = -V.y;
    }
    if (dot(N, V) < 0.0) {
        N = reflect(N, V) * -1.0;
    }
    // our material over the finish, as much of it as asked; its own light
    vec3 albedo = base.rgb;
    vec3 lit = vec3(0.0);
    if (material.look != 0u) {
        float t = view.params.x;
        float s = clamp(material.strength, 0.0, 1.0);
        vec3 c = material.colour.rgb;
        float nv = clamp(dot(N, V), 0.0, 1.0);
        vec3 p = vModel;
        vec3 a2 = albedo;
        float r2 = rough;
        float m2 = metal;
        if (material.look == 1u) {
            // shadow: black as soot, smoke curling up over it (its wisps catching a little light), a pale breath at
            // its edges
            vec3 q = p * 18.0 + vec3(0.0, -t * 0.45, t * 0.15);
            float smoke = fbm(q + fbm(q * 0.7 + t * 0.1) * 1.6);
            float wisp = smoothstep(0.48, 0.72, smoke);
            a2 = mix(vec3(0.004), vec3(0.10), wisp);
            r2 = 1.0;
            m2 = 0.0;
            float breath = 0.7 + 0.3 * sin(t * 2.4);
            lit = c * (pow(1.0 - nv, 3.0) * (0.35 + 0.65 * wisp) * 0.9 * breath + wisp * 0.025);
        } else if (material.look == 2u) {
            // the polymorph (as the Chams' Polymorph, its metal): a mirror of liquid metal, slow ripples running over it
            vec3 q = p * 26.0 + vec3(t * 0.6, t * 0.35, -t * 0.45);
            vec2 w = vec2(fbm(q), fbm(q + vec3(5.2, 1.3, 8.4))) - 0.5;
            N = normalize(N + (T * w.x + B * w.y) * 0.55);
            a2 = c;
            r2 = 0.05;
            m2 = 1.0;
            // a liquid mirror shows the room: a soft box overhead and the windows at its sides, sliding over it
            vec3 R = reflect(-V, N);
            float boxes = pow(clamp(R.y * 0.5 + 0.5, 0.0, 1.0), 10.0) * 1.1 + pow(abs(R.x), 24.0) * 0.6;
            lit = c * boxes * 0.8;
        } else if (material.look == 7u) {
            // the ink (as the Chams' Ink): near black, clouds of its dye turning slowly inside it, thinner in curling
            // streaks where its colour shows; wet: the room's light slides over it, a fine unrest on its surface
            float tt = t * 0.18;
            vec3 q = p * 24.0;
            vec2 w = vec2(fbm(q + vec3(0.0, tt, 0.0)), fbm(q + vec3(5.2, 1.3, 0.0) - tt));
            float swirl = fbm(q + 2.8 * vec3(w, w.x) + vec3(tt * 0.7, -tt, 0.0));
            a2 = mix(vec3(0.003, 0.004, 0.009), c * 0.35, smoothstep(0.38, 0.68, swirl));
            float e = 0.06;
            vec3 pf = p * 60.0 + tt;
            float g = fbm(pf);
            vec2 slope = vec2(fbm(pf + vec3(e, 0.0, 0.0)) - g, fbm(pf + vec3(0.0, e, 0.0)) - g) / e;
            // (none of the unrest where it is seen edge on: there the least tilt swings its mirror)
            N = normalize(N + (T * slope.x + B * slope.y) * 0.025 * smoothstep(0.1, 0.5, nv));
            r2 = 0.12;
            m2 = 0.0;
            lit = c * 0.6 * pow(smoothstep(0.58, 0.82, swirl), 2.0) + c * 0.12 * pow(1.0 - nv, 2.0);
        } else if (material.look == 8u) {
            // the curse (as the Chams' Cursed): pale dead flesh, the finish's light and dark pressed into it, blotched;
            // through it dark vessels — trunks and finer branches off them — crawling as their warp drifts, a slow beat
            // along them now and then; matte, a pale cold rim where it turns away
            float tt = t * 0.045;
            float lum = dot(albedo, LUMA);
            vec3 flesh = c * mix(0.72, 1.25, sqrt(clamp(lum, 0.0, 1.0)));
            flesh *= mix(0.82, 1.1, fbm(p * 14.0 + 3.1)) * mix(0.93, 1.05, fbm(p * 110.0 + 7.7));
            vec3 w = vec3(fbm(p * 7.0 + vec3(tt, 0.0, 0.0)), fbm(p * 7.0 + vec3(3.7, -tt, 1.9)), fbm(p * 7.0 + vec3(-tt, 2.3, 5.1))) - 0.5;
            vec2 c1 = cells(p * 22.0 + w * 2.2);
            vec2 c2 = cells(p * 55.0 + w * 3.1 + 11.0);
            float thick = mix(0.05, 0.13, noise(p * 12.0 + 4.0));
            float trunk = 1.0 - smoothstep(0.25 * thick, thick, c1.x);
            float near = 1.0 - smoothstep(0.0, 0.45, c1.x);
            float branch = (1.0 - smoothstep(0.0175, 0.07, c2.x)) * smoothstep(0.1, 0.9, near + 0.25 * noise(p * 26.0));
            float vessels = clamp(trunk + 0.7 * branch, 0.0, 1.0);
            float beat = pow(0.5 + 0.5 * sin(t * 1.7 - dot(p, vec3(8.0, 4.5, 3.0)) * 1.2), 10.0);
            float dark = clamp(vessels * (0.75 + 0.25 * beat), 0.0, 1.0);
            a2 = mix(flesh * (1.0 - near * 0.3), vec3(0.03, 0.012, 0.012), dark);
            r2 = 0.9;
            m2 = 0.0;
            lit = vec3(0.09, 0.095, 0.1) * pow(1.0 - nv, 3.0) * (1.0 - dark);
        } else if (material.look == 6u) {
            // its own colour: the finish's light and shade kept, its colour this
            float l = dot(albedo, LUMA);
            a2 = clamp(c * l / max(dot(c, LUMA), 0.04), 0.0, 1.0);
        }
        albedo = mix(albedo, a2, s);
        rough = mix(rough, r2, s);
        metal = mix(metal, m2, s);
        lit *= s;
    }
    vec3 col = shade(albedo, N, V, rough, metal, ao, cloth);
    col += lit * view.horizon.w;
    // StatTrak's digits light themselves (g_flSelfIllumBrightness 1.2)
    col += base.rgb * illum * view.horizon.w;
    // whole, or (the page's look coming in) as much as is seen yet; a reflection faint, fading away under the floor
    float seen = 1.0 - view.params.y;
    if (mirror) {
        seen *= 0.34 * exp(-under / max(view.params.w, 1e-3));
    }
    outColor = vec4(present(col), seen);
}
