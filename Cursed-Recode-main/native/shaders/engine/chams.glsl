// Chams (cursed.recode.chams.Chams): a body in one of its looks — ink (CURSED_CHAMS 2), the curse (3), the
// polymorphic liquid (4, its surface where it is not drawn by the engine's own pass), the shadow (5); anything else
// drawn as the game draws it.
// It is in the creature's own fragment shader (EntityEngine), after its texture is read: cursedPos (from the camera, on
// the world's axes), cursedNormal, texCoord0, chamsFace (where on its face, 0..1 each way), chamsOverlay (the overlay
// as given: the hurt flash below 8 in y), chamsRot (the world's axes to the view's), chamsProj (the projection's
// scale), CursedScene (the picture just before the first chams of the pass: what is behind them, vk/pass.rs); the
// engine's CursedChamsTint, CursedChamsLook and CursedChamsMore (the look as the player set it, engine/ink.rs Look).

// An integer hash of a lattice point (no pattern of its own), 0..1.
float ch_uhash(vec2 i) {
    uvec2 q = uvec2(ivec2(i) + 32768);
    uint h = (q.x * 1597334677u) ^ (q.y * 3812015801u);
    h *= 0x27d4eb2du;
    h ^= h >> 15;
    h *= 0x2c1b3c6du;
    h ^= h >> 12;
    return float(h) * (1.0 / 4294967296.0);
}

// Gradient noise: a slope at each lattice point, not a value — cut sharply it shows no squares and corners (the value
// noise's lattice does). Spread 0..1 as a value noise is (its scale measured).
float ch_gnoise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    vec2 u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    float g[4];
    for (int k = 0; k < 4; k++) {
        vec2 o = vec2(k & 1, k >> 1);
        float a = ch_uhash(i + o) * 6.2831853;
        g[k] = dot(vec2(cos(a), sin(a)), f - o);
    }
    return 0.5 + 1.05 * mix(mix(g[0], g[1], u.x), mix(g[2], g[3], u.x), u.y);
}

// Its five octaves, each finer than about two pixels giving way to its mean: `fw` how much of the noise one pixel spans
// (a face seen nearly edge on squeezes its pattern finer than the pixels, and the finest octaves only sparkle there).
float ch_gfbm(vec2 p, float fw) {
    float s = 0.0;
    float a = 0.5;
    float m = 1.0;
    mat2 turn = mat2(0.8, 0.6, -0.6, 0.8);
    for (int i = 0; i < 5; i++) {
        s += a * mix(0.5, ch_gnoise(p), 1.0 - smoothstep(0.25, 0.5, fw * m));
        p = turn * p * 2.03 + 17.1;
        a *= 0.5;
        m *= 2.03;
    }
    return s;
}

// How much of a pattern's coordinates one pixel spans (the larger way).
float ch_span(vec2 q) {
    vec2 w = fwidth(q);
    return max(w.x, w.y);
}

float ch_seconds() {
    return CursedTime.z;
}

vec2 ch_size() {
    return vec2(textureSize(CursedScene, 0));
}

vec2 ch_screen() {
    return gl_FragCoord.xy / ch_size();
}

vec3 ch_scene(vec2 uv) {
    return textureLod(CursedScene, clamp(uv, vec2(0.0005), vec2(0.9995)), 0.0).rgb;
}

// A direction on the world's axes as a step on the screen (y down, as gl_FragCoord): how a reflection or a bend
// moves what is seen.
vec2 ch_step(vec3 dir) {
    vec3 v = chamsRot * dir;
    return vec2(v.x * chamsProj.x, -v.y * chamsProj.y) * 0.5;
}

// How much of a step stays on the screen: fading to nothing at its edges.
float ch_inside(vec2 uv) {
    vec2 e = min(uv, 1.0 - uv);
    return smoothstep(0.0, 0.06, min(e.x, e.y));
}

// The face a point is on: its two directions (blocks per 0..1 of chamsFace, from the screen's derivatives: a face is
// flat, so they are the same all over it), and how far the point is from its nearest edges, in blocks and in pixels.
struct ChFace {
    vec3 u;
    vec3 v;
    vec2 edge;
    vec2 edgePx;
    vec2 side;
    bool ok;
};

ChFace ch_face() {
    ChFace f;
    vec3 dpdx = dFdx(cursedPos);
    vec3 dpdy = dFdy(cursedPos);
    vec2 dfdx = dFdx(chamsFace);
    vec2 dfdy = dFdy(chamsFace);
    float det = dfdx.x * dfdy.y - dfdx.y * dfdy.x;
    f.ok = abs(det) > 1e-9;
    f.u = f.ok ? (dpdx * dfdy.y - dpdy * dfdx.y) / det : vec3(0.0);
    f.v = f.ok ? (dpdy * dfdx.x - dpdx * dfdy.x) / det : vec3(0.0);
    vec2 to = 0.5 - abs(chamsFace - 0.5);
    f.edge = to * vec2(length(f.u), length(f.v));
    f.edgePx = to / max(fwidth(chamsFace), vec2(1e-6));
    f.side = sign(chamsFace - 0.5);
    return f;
}

// The face's normal rolled over toward its edges within `width` blocks, as if the box were a soft body: what makes
// wet ink shine along a body's edges.
vec3 ch_round(vec3 N, ChFace f, float width) {
    if (!f.ok) {
        return N;
    }
    // never past four tenths of the face (a thin face's two sides meet smoothly in its middle)
    vec2 w = min(vec2(width), 0.4 * vec2(length(f.u), length(f.v)));
    vec2 k = 1.0 - smoothstep(vec2(0.0), w, f.edge);
    k = k * k * 1.6;
    vec3 n = N + normalize(f.u) * f.side.x * k.x + normalize(f.v) * f.side.y * k.y;
    return normalize(n);
}

// The world round, seen along R, as a wet surface mirrors it: the sky deeper overhead, bright at the horizon (a band of
// light there, as round chrome in a studio), dark ground under it; the sun small and hard.
vec3 ch_sky(vec3 R) {
    vec3 sky = CursedSky.rgb;
    vec3 horizon = FogColor.rgb;
    float y = R.y;
    vec3 c = mix(horizon * 1.08 + 0.05, sky * 0.7, smoothstep(0.0, 0.8, y));
    c += (horizon + 0.15) * 0.5 * exp(-(y * y) / 0.012);
    vec3 ground = mix(horizon * 0.42 + 0.03, horizon * 0.16 + 0.025, smoothstep(-0.04, -0.6, y));
    c = mix(c, ground, smoothstep(0.015, -0.06, y));
    if (CursedSun.w > 0.0) {
        float s = max(dot(R, CursedSun.xyz), 0.0);
        c += CursedSunColor.rgb * (pow(s, 1400.0) * 30.0 + pow(s, 70.0) * 0.5) * CursedSun.w;
    }
    return c;
}

// The light the game gives the body here (block and sky), as one number: ink goes dark in a cave too.
float ch_light(vec4 light) {
    return clamp(max(light.r, max(light.g, light.b)), 0.0, 1.0);
}

// A height over a face (blocks) as its normal bent: the height's slope along the face's two directions (blocks per
// block), from the height itself a little aside on the face (no derivatives of it: they break along triangles' edges).
vec3 ch_tilt(vec3 N, ChFace f, vec2 slope) {
    if (!f.ok) {
        return N;
    }
    return normalize(N - normalize(f.u) * slope.x - normalize(f.v) * slope.y);
}

// Where on its face a point is in blocks, and which way is down along the face (in the face's two directions).
vec2 ch_on_face(ChFace f) {
    return chamsFace * vec2(length(f.u), length(f.v));
}

// Which face this is, as a place to start its pattern from: the middle of the face on its texture (the skin's
// coordinates move with the body). A seed from the face's direction in the world would slide its pattern every frame
// a limb swings or the body turns.
vec2 ch_face_seed() {
    mat2 F = mat2(dFdx(chamsFace), dFdy(chamsFace));
    if (abs(determinant(F)) < 1e-12) {
        return vec2(0.0);
    }
    mat2 J = mat2(dFdx(texCoord0), dFdy(texCoord0)) * inverse(F);
    return (texCoord0 + J * (0.5 - chamsFace)) * 40.0;
}

// What wet ink mirrors along R: the sky, and only a little of the picture where it lands, blurred wide — a limb's flat
// face would show a sharp photograph of the ground in it, cut square at its edges.
vec3 ch_env_wet(vec3 R, vec3 V, float reach) {
    vec3 env = ch_sky(R);
    vec2 at = ch_screen() + ch_step(R) * reach;
    float w = ch_inside(at) * (1.0 - smoothstep(0.15, 0.75, dot(R, V))) * (1.0 - smoothstep(0.06, 0.4, R.y));
    vec2 px = 9.0 / ch_size();
    vec3 seen = vec3(0.0);
    for (int k = 0; k < 8; k++) {
        float a = float(k) * 0.785398 + 0.39;
        seen += ch_scene(at + vec2(cos(a), sin(a)) * px * ((k & 1) == 0 ? 1.0 : 0.5));
    }
    return mix(env, seen * 0.125, w * 0.4);
}

// The ink: near black, clouds of its dye (deep blue as made) turning slowly inside it, wet: the sky's light sliding over
// its rolled edges. Its pattern stays on each face as the body moves (ch_face_seed), of gradient noise (no lattice in its
// folds). The player's dye tints it (CursedChamsTint: 1 as made), its shine and how fast its pattern flows
// (CursedChamsLook x, y: 1 as made).
vec4 ch_ink(vec4 base, float light, float hurt) {
    ChFace f = ch_face();
    vec3 N0 = normalize(cursedNormal);
    vec3 V = normalize(-cursedPos);
    vec3 tint = CursedChamsTint.rgb;
    float shine = CursedChamsLook.x;
    float t = ch_seconds() * 0.18 * CursedChamsLook.y;
    vec2 seed = ch_face_seed();
    vec2 q = ch_on_face(f) * 3.2 + seed;
    float fq = ch_span(q);
    vec2 w = vec2(ch_gfbm(q + vec2(0.0, t), fq), ch_gfbm(q + vec2(5.2, 1.3) - t, fq));
    // the warp stretches the pattern it reads: half as much again of it under a pixel
    float swirl = ch_gfbm(q + 2.8 * w + vec2(t * 0.7, -t), fq * 1.5);
    // the ink thick and black, thinner in curling streaks where it shows its blue
    vec3 col = mix(vec3(0.003, 0.004, 0.009), vec3(0.05, 0.07, 0.16), smoothstep(0.38, 0.68, swirl)) * tint;
    col += vec3(0.1, 0.14, 0.3) * tint * pow(smoothstep(0.58, 0.82, swirl), 2.0);
    vec3 N = ch_round(N0, f, 0.08);
    // a fine unrest on its surface, on the face's own coordinates
    vec2 pf = ch_on_face(f) * 6.0 + seed * 1.7;
    float fp = ch_span(pf);
    float e = 0.06;
    float g = ch_gfbm(pf + t, fp);
    // gentle, and none on a face seen nearly edge on: there the least tilt swings the mirror between the sky and the
    // ground, and the face shows bright cells
    float unrest = 0.035 * smoothstep(0.1, 0.5, abs(dot(N0, V)));
    N = ch_tilt(N, f, vec2(ch_gfbm(pf + vec2(e, 0.0) + t, fp) - g, ch_gfbm(pf + vec2(0.0, e) + t, fp) - g) / e * unrest);
    vec3 R = reflect(-V, N);
    float cosv = max(dot(N, V), 0.0);
    float fres = 0.035 + 0.965 * pow(1.0 - cosv, 5.0);
    // thinner where it turns from the eye: a little of its blue there
    col = mix(col, vec3(0.04, 0.06, 0.14) * tint, pow(1.0 - cosv, 2.0) * 0.6);
    // what it mirrors, a little blue (a dye's surface, not a mirror's), never white along its edges
    col += min(ch_env_wet(R, V, 0.2) * vec3(0.8, 0.86, 1.0) * fres * 0.6 * shine, vec3(0.38 * shine));
    if (CursedSun.w > 0.0) {
        col += CursedSunColor.rgb * pow(max(dot(R, CursedSun.xyz), 0.0), 260.0) * shine * CursedSun.w;
    }
    col += vec3(0.1, 0.12, 0.18) * tint * hurt * smoothstep(0.6, 1.0, swirl);
    return vec4(col * mix(0.35, 1.0, light), 1.0);
}

// The cells of a jittered lattice round p: the distance to the nearest point and to the next (F1, F2), and the
// nearest's own number. F2 - F1 is 0 along the borders between cells: a network of lines branching as vessels and
// cracks do.
vec3 ch_cells(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    float d1 = 8.0;
    float d2 = 8.0;
    float id = 0.0;
    for (int y = -1; y <= 1; y++) {
        for (int x = -1; x <= 1; x++) {
            vec2 o = vec2(x, y);
            float h = ch_uhash(i + o);
            vec2 c = o + 0.5 + 0.42 * vec2(cos(h * 6.2831853), sin(h * 6.2831853 + 1.3)) - f;
            float d = dot(c, c);
            if (d < d1) {
                d2 = d1;
                d1 = d;
                id = h;
            } else if (d < d2) {
                d2 = d;
            }
        }
    }
    return vec3(sqrt(d1), sqrt(d2), id);
}

// A line of the network, 1 on it, `width` (of the lattice's cells) wide, never thinner than about a pixel (`fw` the
// lattice's span over one).
float ch_vessel(vec2 q, float width, float fw) {
    vec3 c = ch_cells(q);
    float w = max(width, fw * 1.2);
    return 1.0 - smoothstep(0.25 * w, w, c.y - c.x);
}

// The curse: flesh gone to pale ash (the skin's own light and dark pressed into it, as a carving keeps its lines),
// dead and blotched; through it a network of dark vessels — thick trunks, fine branches off them, warped as living
// ones are, crawling as the warp drifts — and the rot: dark patches with hard edges spreading and drawing back slowly
// over it, the vessels thickest in them. Now and then a slow beat goes along the vessels. Matte and cold: a pale rim
// where it turns from the eye, its edges sunk. CursedChamsTint the flesh's colour (linear), Look x how dark, y how
// fast the vessels crawl, z the vessels on (1) or off (0), w how strongly they beat; More[0] the vessels' colour (the
// strands writhing out of them are engine/curse.rs's).
vec4 ch_cursed(vec4 base, float light, float hurt) {
    ChFace f = ch_face();
    vec3 N0 = normalize(cursedNormal);
    vec3 V = normalize(-cursedPos);
    float darkness = CursedChamsLook.x;
    float t = ch_seconds() * 0.045 * CursedChamsLook.y;
    vec2 seed = ch_face_seed();
    vec2 p = ch_on_face(f) * 2.4 + seed;
    float fp = ch_span(p);
    // the skin pressed into a narrow pale grey: its features still there, all of one dead colour
    float lum = dot(base.rgb, vec3(0.2126, 0.7152, 0.0722));
    vec3 flesh = CursedChamsTint.rgb * mix(0.72, 1.25, sqrt(clamp(lum, 0.0, 1.0)));
    // blotched: paler and greyer patches, a fine grain of dead skin
    float blot = ch_gfbm(p * 1.3 + 3.1, fp * 1.3);
    float grain = ch_gfbm(p * 9.0 + 7.7, fp * 9.0);
    flesh *= mix(0.82, 1.1, blot) * mix(0.93, 1.05, grain);
    float dark = 0.0;
    float bruise = 0.0;
    float vessels = 0.0;
    if (CursedChamsLook.z > 0.5) {
        // the warp the vessels lie in, drifting: they crawl
        vec2 w = vec2(ch_gfbm(p * 0.55 + vec2(t, 0.0), fp * 0.55), ch_gfbm(p * 0.55 + vec2(3.7, -t), fp * 0.55)) - 0.5;
        vec2 q1 = p * 1.6 + 2.2 * w;
        vec2 q2 = p * 4.1 + 3.1 * w + 11.0;
        float fq1 = ch_span(q1);
        float fq2 = ch_span(q2);
        // the trunks a little thicker in some places than others; the branches finer, only near the trunks
        float thick = mix(0.05, 0.13, ch_gnoise(p * 0.9 + 4.0));
        float trunk = ch_vessel(q1, thick, fq1);
        float near = 1.0 - smoothstep(0.0, 0.45, ch_cells(q1).y - ch_cells(q1).x);
        float branch = ch_vessel(q2, 0.07, fq2) * smoothstep(0.1, 0.9, near + 0.25 * ch_gnoise(p * 2.0));
        vessels = clamp(trunk + 0.7 * branch, 0.0, 1.0);
        // the rot: hard-edged dark patches spreading and drawing back
        float rotn = ch_gfbm(p * 0.42 + vec2(-t * 1.6, t * 0.9), fp * 0.42);
        float rot = smoothstep(0.66, 0.7, rotn + 0.05 * grain);
        float edge = smoothstep(0.6, 0.66, rotn) - rot;
        dark = clamp(vessels * (0.7 + 0.6 * rot) + rot * 0.72 + edge * 0.25, 0.0, 1.0);
        // round a trunk the flesh is bruised
        bruise = near * 0.3 + edge * 0.2;
        // the beat: slow, now and then, along them
        float beat = pow(0.5 + 0.5 * sin(ch_seconds() * 1.7 - dot(p, vec2(0.8, 0.45)) * 1.2), 10.0) * CursedChamsLook.w;
        dark = clamp(dark + vessels * beat * 0.3, 0.0, 1.0);
    }
    vec3 col = flesh * (1.0 - bruise * darkness);
    col = mix(col, CursedChamsMore[0].rgb, clamp(dark * darkness, 0.0, 1.0));
    // its edges sunk, as the cavities of dead flesh are
    float e = f.ok ? min(f.edge.x, f.edge.y) : 1.0;
    col *= mix(0.6, 1.0, smoothstep(0.0, 0.07, e));
    // matte: no mirror; a pale cold rim where it turns from the eye, none on what is dark
    vec3 N = ch_round(N0, f, 0.05);
    float cosv = max(dot(N, V), 0.0);
    col += vec3(0.09, 0.095, 0.1) * pow(1.0 - cosv, 3.0) * (1.0 - dark);
    // hurt: it goes darker, not red
    col *= 1.0 - 0.45 * hurt;
    return vec4(col * mix(0.32, 1.0, light), 1.0);
}

// The polymorphic liquid on polygons (the first person's hands, what a body wears and holds: the body itself the
// engine draws as one liquid with its drops, engine/liquid.rs): its surface rolled at the edges, a slow ripple over it,
// the liquid as the player made it (as engine/liquid.rs draws it): what is behind it bent through it and taken toward
// its depth's colour as much as it is clear, the world mirrored in it (a metal's tinted), its rim, its rainbow film,
// the sun's highlight as sharp as it is smooth. CursedChamsTint its colour (linear), w how clear; Look x how much it
// mirrors, z how restless, w how much of a metal; More[0] its depth's colour, w how much it bends; More[1] its
// highlights' colour, w its film; More[2] its rim's colour, w how rough.
vec4 ch_liquid(vec4 base, float light, float hurt) {
    ChFace f = ch_face();
    vec3 N0 = normalize(cursedNormal);
    vec3 V = normalize(-cursedPos);
    vec3 N = ch_round(N0, f, 0.12);
    vec2 q = ch_on_face(f) * 4.0 + ch_face_seed();
    float fq = ch_span(q);
    float t = ch_seconds() * 0.5;
    float e = 0.05;
    float g = ch_gfbm(q + t, fq);
    N = ch_tilt(N, f, vec2(ch_gfbm(q + vec2(e, 0.0) + t, fq) - g, ch_gfbm(q + vec2(0.0, e) + t, fq) - g) / e * 0.06 * max(CursedChamsLook.z, 0.2));
    float cosv = max(dot(N, V), 0.0);
    vec3 R = reflect(-V, N);
    vec3 tint = CursedChamsTint.rgb;
    float clear = clamp(CursedChamsTint.w, 0.0, 1.0);
    float mirror_k = CursedChamsLook.x;
    float metal = clamp(CursedChamsLook.w, 0.0, 1.0);
    float rough = clamp(CursedChamsMore[2].w, 0.0, 1.0);
    float eta = 1.0 + 0.33 * CursedChamsMore[0].w;
    float f0d = pow((eta - 1.0) / (eta + 1.0), 2.0);
    vec3 F0 = mix(vec3(f0d), tint, metal);
    vec3 F = F0 + (1.0 - F0) * pow(1.0 - cosv, 5.0) * mix(1.0, 0.45, rough);
    vec3 env = ch_env_wet(R, V, 0.25 + rough * 0.3);
    env *= mix(vec3(1.0), CursedChamsMore[2].rgb * 1.25, pow(1.0 - cosv, 3.0) * 0.7);
    float film = CursedChamsMore[1].w;
    if (film > 0.0) {
        float th = cosv * 2.2 + ch_gnoise(q * 1.5 + t * 0.3) * 0.9;
        env *= mix(vec3(1.0), (0.5 + 0.5 * cos(6.2831853 * (th + vec3(0.0, 0.33, 0.67)))) * 1.7, film * 0.85);
    }
    vec3 mirror = env * F * mirror_k;
    // through it: what is behind bent, taken toward its depth's colour (a hand's thickness), its own colour lit inside
    vec3 behind = ch_scene(ch_screen() + ch_step(-N + V * 0.4) * 0.05 * CursedChamsMore[0].w);
    vec3 deep = clamp(CursedChamsMore[0].rgb, vec3(0.004), vec3(0.996));
    vec3 T = exp(log(deep) * (0.25 + 6.0 * (1.0 - clear)) * 0.08) * clear;
    vec3 inner = tint * (0.18 + 0.5 * light);
    vec3 through = (behind * T * mix(vec3(1.0), tint * 1.15, 0.3) + inner * (1.0 - clamp(T, 0.0, 1.0)) * (1.0 - clear * 0.6)) * (1.0 - F * mirror_k * 0.9);
    vec3 col = mix(through + mirror, mirror, metal);
    if (CursedSun.w > 0.0) {
        float sharp = mix(500.0, 12.0, rough);
        col += CursedSunColor.rgb * CursedChamsMore[1].rgb * pow(max(dot(R, CursedSun.xyz), 0.0), sharp) * mix(1.5, 0.3, rough) * mirror_k * CursedSun.w;
    }
    col *= 1.0 - 0.35 * hurt;
    return vec4(col * mix(0.45, 1.0, light), 1.0);
}

// The shadow: the body thick black smoke — near black, a slow smoke flowing and curling over it (its thin places a
// shade paler, the sky just touching its edges as they turn away); its eyes thin straight slits of light where that
// creature has them (cursed.recode.chams.EyeMaps marks their texels in its texture's alpha, each carrying its eye's
// slit in its colour), a white-hot line in the eyes' colour, soft light round it on its face, breathing slowly,
// shutting now and then. The smoke fraying off it and trailing behind it is engine/shadow.rs's. CursedChamsTint the
// eyes' colour (linear), w 1 they glow; Look x how much smoke (1 as made).
const float CH_EYE = 253.0;

// How far `p` (texel coordinates on the texture) is from the nearest eye's slit, in texels (99 none within reach): an
// eye texel's red and green are its slit's ends across, its blue its middle down, each in half texels from the texel's
// own corner (128 at it).
float ch_slit(vec2 p) {
    ivec2 tsz = textureSize(Sampler0, 0);
    ivec2 c = ivec2(floor(p));
    float d = 99.0;
    for (int dy = -2; dy <= 2; dy++) {
        for (int dx = -3; dx <= 3; dx++) {
            ivec2 q = clamp(c + ivec2(dx, dy), ivec2(0), tsz - 1);
            vec4 e = texelFetch(Sampler0, q, 0);
            if (abs(e.a * 255.0 - CH_EYE) < 0.5) {
                vec3 k = floor(e.rgb * 255.0 + 0.5) - 128.0;
                vec2 a = vec2(q) + vec2(k.x, k.z) * 0.5;
                float len = (k.y - k.x) * 0.5;
                // its ends drawn in a little, rounded
                float x = clamp(p.x - a.x, min(0.3, len * 0.5), max(len - 0.3, len * 0.5));
                d = min(d, length(vec2(p.x - a.x - x, p.y - a.y)));
            }
        }
    }
    return d;
}

vec4 ch_shadow(vec4 base, float light, float hurt) {
    vec3 eye_col = CursedChamsTint.rgb;
    bool eyes = CursedChamsTint.w > 0.5;
    ChFace f = ch_face();
    vec3 N0 = normalize(cursedNormal);
    vec3 V = normalize(-cursedPos);
    vec2 p = ch_on_face(f) * 2.6 + ch_face_seed();
    float fp = ch_span(p);
    float t = ch_seconds();
    float amount = clamp(CursedChamsLook.x, 0.0, 3.0);
    // the smoke flowing over it, curling through itself
    vec2 w = vec2(ch_gfbm(p * 0.7 + vec2(0.0, t * 0.12), fp * 0.7), ch_gfbm(p * 0.7 + vec2(5.2, -t * 0.1), fp * 0.7)) - 0.5;
    float smoke = ch_gfbm(p * 1.6 + 2.0 * w + vec2(0.0, -t * 0.35), fp * 1.6);
    float grain = ch_gfbm(p * 6.0 + w + vec2(0.0, -t * 0.6), fp * 6.0);
    float thin = smoothstep(0.45, 0.85, smoke) * (0.6 + 0.4 * grain);
    vec3 col = vec3(0.012, 0.012, 0.014) + vec3(0.05, 0.052, 0.058) * thin * (0.4 + 0.6 * min(amount, 1.5));
    float cosv = max(dot(N0, V), 0.0);
    col += vec3(0.05, 0.055, 0.065) * pow(1.0 - cosv, 3.0) * (0.3 + 0.7 * thin) * light;
    if (eyes) {
        vec2 tp = texCoord0 * vec2(textureSize(Sampler0, 0));
        float d = ch_slit(tp);
        if (d < 4.0) {
            // one body's eyes breathe together (a seed from where it stands: not every creature at once), slowly, and
            // shut for a moment every seven seconds or so
            vec3 at = vec3(CursedCameraBlock.xyz) + CursedCameraFrac.xyz + cursedPos;
            float seed = ch_uhash(floor(at.xz / 3.0));
            float breath = 0.8 + 0.2 * sin((t / 2.6 + seed) * 6.2831853);
            float cyc = t / 7.0 + seed * 7.0;
            float b = (fract(cyc) - 0.15 - 0.6 * ch_uhash(vec2(floor(cyc), seed * 977.0))) / 0.03;
            float open = 1.0 - (b > 0.0 && b < 1.0 ? sin(b * 3.1415927) : 0.0);
            // the line: a tenth of a texel each side of its middle, never under about a pixel (far off it still glows)
            float px = ch_span(tp);
            float half_w = max(0.13 * open, 0.55 * px);
            float line = (1.0 - smoothstep(half_w - px * 0.5, half_w + px * 0.5, d)) * max(open, 0.0);
            float glow = exp(-d * 1.7) * (0.25 + 0.75 * open);
            float flare = 1.0 + 0.6 * hurt;
            col += eye_col * glow * 0.6 * breath * flare;
            vec3 hot = mix(eye_col, vec3(1.0), 0.55) * (0.25 + breath) * flare;
            col = mix(col, min(hot, vec3(1.25)), line);
        }
    }
    return vec4(col, 1.0);
}

// The body's colour in its matter: `base` its texture's colour (cut where it is clear), `lit` the game's final colour,
// `light` the game's light here.
vec4 cursed_chams(vec4 base, vec4 lit, vec4 light) {
    float l = ch_light(light);
    float hurt = chamsOverlay.y < 8 ? 1.0 : 0.0;
#if CURSED_CHAMS == 2
    return ch_ink(base, l, hurt);
#elif CURSED_CHAMS == 3
    return ch_cursed(base, l, hurt);
#elif CURSED_CHAMS == 4
    return ch_liquid(base, l, hurt);
#elif CURSED_CHAMS == 5
    return ch_shadow(base, l, hurt);
#elif CURSED_CHAMS == 99
    // dev: where on its face (red, green), which corner the vertex was (blue: the face's own derivatives)
    return vec4(chamsFace, fract(texCoord0.x * 16.0) * 0.3, 1.0);
#else
    return lit;
#endif
}
