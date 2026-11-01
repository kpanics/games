// The sky's shape: sunlight scattered once by the air (Rayleigh) and by haze (Mie) on its way to the eye, through an
// atmosphere of the Earth's size. Taken relative to the sky straight overhead, so the game keeps its own colours (day,
// night, rain, effects) and the sky gets its form: a deeper blue up high, paler toward the horizon, a bright halo round
// the sun, red and orange low toward a setting sun. Worked out once a frame into a table (sky_lut.comp: the palette's
// rows 258..321, by the angle from the sun round the horizon and the height above it); the sky's pixels read the table
// (twins of the game's core/sky, cursed.recode.render.EntityEngine). Needs CursedEngine declared before.

const float CA_EARTH = 6360e3;
const float CA_TOP = 6420e3;
const vec3 CA_RAYLEIGH = vec3(5.8e-6, 13.5e-6, 33.1e-6);
const float CA_MIE = 21e-6;
const float CA_HR = 8000.0;
const float CA_HM = 1200.0;
const float CA_PI = 3.14159265;
// The table: 256 angles from the sun (0..pi) by 64 heights (0..pi/2, more of them near the horizon).
const int SKY_LUT_ROW = 258;
const ivec2 SKY_LUT_SIZE = ivec2(256, 64);

// How far a ray from `o` (from the Earth's centre) along `d` goes before it leaves the sphere of radius `r`.
float ca_exit(vec3 o, vec3 d, float r) {
    float b = dot(o, d);
    float h = b * b - (dot(o, o) - r * r);
    return h < 0.0 ? 0.0 : -b + sqrt(h);
}

// The light the air sends toward the eye along `dir` with the sun toward `sun` (relative units).
vec3 cursed_atmosphere(vec3 dir, vec3 sun) {
    vec3 o = vec3(0.0, CA_EARTH + 100.0, 0.0);
    const int N = 8;
    const int M = 4;
    float seg = ca_exit(o, dir, CA_TOP) / float(N);
    float mu = dot(dir, sun);
    float phaseR = 0.0596831 * (1.0 + mu * mu);
    const float g = 0.76;
    float phaseM = 0.1193662 * (1.0 - g * g) * (1.0 + mu * mu) / ((2.0 + g * g) * pow(1.0 + g * g - 2.0 * g * mu, 1.5));
    vec3 sumR = vec3(0.0);
    vec3 sumM = vec3(0.0);
    float depthR = 0.0;
    float depthM = 0.0;
    for (int i = 0; i < N; i++) {
        vec3 p = o + dir * (seg * (float(i) + 0.5));
        float h = length(p) - CA_EARTH;
        float hr = exp(-h / CA_HR) * seg;
        float hm = exp(-h / CA_HM) * seg;
        depthR += hr;
        depthM += hm;
        // The sunlight reaching this point, unless the Earth is in the way.
        float segL = ca_exit(p, sun, CA_TOP) / float(M);
        float lightR = 0.0;
        float lightM = 0.0;
        bool lit = true;
        for (int j = 0; j < M; j++) {
            float hl = length(p + sun * (segL * (float(j) + 0.5))) - CA_EARTH;
            if (hl < 0.0) {
                lit = false;
                break;
            }
            lightR += exp(-hl / CA_HR) * segL;
            lightM += exp(-hl / CA_HM) * segL;
        }
        if (lit) {
            vec3 through = exp(-(CA_RAYLEIGH * (depthR + lightR) + CA_MIE * 1.1 * (depthM + lightM)));
            sumR += through * hr;
            sumM += through * hm;
        }
    }
    return sumR * CA_RAYLEIGH * phaseR + sumM * CA_MIE * phaseM;
}

// What the game's sky colour is multiplied by along `d` (y >= 0) with the sun toward `sun`, `up` the air straight up:
// brightness and hue apart — the brightness against overhead pressed down above 1 (the halo round the sun stays a
// glow), the hue's shift as the air gives it (red low toward the sun); close to the sun, where the game draws its own
// glow and disc, kept from washing out into white.
vec3 cursed_sky_ratio(vec3 d, vec3 sun, vec3 up) {
    vec3 here = cursed_atmosphere(d, sun);
    const vec3 LUMA = vec3(0.2126, 0.7152, 0.0722);
    float lumHere = max(dot(here, LUMA), 1e-12);
    float lumUp = max(dot(up, LUMA), 1e-12);
    float bright = lumHere / lumUp;
    bright = bright < 1.0 ? max(bright, 0.3) : 1.0 + (bright - 1.0) / (1.0 + (bright - 1.0) / 0.25);
    vec3 hue = clamp((here / lumHere) / (up / lumUp), vec3(0.4), vec3(2.5));
    float nearSun = smoothstep(0.9, 0.995, dot(d, sun));
    bright = mix(bright, min(bright, 1.0), nearSun);
    hue = mix(hue, mix(vec3(1.0), hue, 0.5), nearSun);
    return hue * bright;
}

#ifdef CURSED_SKY_LUT
// The game's sky colour `game` along `dir`, shaped by the air (flag 1024), read from the table.
vec3 cursed_sky_shape(vec3 game, vec3 dir) {
    vec3 sun = CursedSun.xyz;
    // The sun's light in the sky, down into the dusk after it sets.
    float day = smoothstep(-0.3, 0.02, sun.y) * (1.0 - CursedSky.a);
    if ((CursedSize.z & 1024) == 0 || day <= 0.0) {
        return game;
    }
    vec3 d = normalize(dir);
    float height = asin(clamp(d.y, 0.0, 1.0));
    float v = sqrt(height / (0.5 * CA_PI));
    vec2 sunH = length(sun.xz) > 1e-4 ? normalize(sun.xz) : vec2(1.0, 0.0);
    vec2 dH = length(d.xz) > 1e-4 ? normalize(d.xz) : sunH;
    float u = acos(clamp(dot(dH, sunH), -1.0, 1.0)) / CA_PI;
    // By hand between the four nearest entries (the palette is read without filtering).
    vec2 at = clamp(vec2(u, v) * vec2(SKY_LUT_SIZE) - 0.5, vec2(0.0), vec2(SKY_LUT_SIZE - 1));
    ivec2 i = ivec2(floor(at));
    vec2 f = at - vec2(i);
    ivec2 j = min(i + 1, SKY_LUT_SIZE - 1);
    vec3 a = texelFetch(CursedPalette, ivec2(i.x, SKY_LUT_ROW + i.y), 0).rgb;
    vec3 b = texelFetch(CursedPalette, ivec2(j.x, SKY_LUT_ROW + i.y), 0).rgb;
    vec3 c = texelFetch(CursedPalette, ivec2(i.x, SKY_LUT_ROW + j.y), 0).rgb;
    vec3 e = texelFetch(CursedPalette, ivec2(j.x, SKY_LUT_ROW + j.y), 0).rgb;
    vec3 ratio = mix(mix(a, b, f.x), mix(c, e, f.x), f.y);
    return game * mix(vec3(1.0), ratio, day);
}
#endif
