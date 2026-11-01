// Shared head of the launch's shaders (launch/mod.rs): the frame's numbers and the noise everything is drawn from.
// Ported from the approved web preview (Desktop\Cursed Recode Launch\src.html); the bodies keep its names (u…),
// mapped onto one uniform block. Pictures are stored row 0 at the top; the math runs y up, as the preview did.

layout(std140, set = 0, binding = 0) uniform Launch {
    vec4 Cam;     // camera, tan of half the vertical view
    vec4 Right;   // right, aspect
    vec4 Upv;     // up, time (the fire's, the water's: always running)
    vec4 Fwd;     // forward, the height map's z origin
    vec4 LC;      // the logo's centre, its size
    vec4 LR;      // its right
    vec4 LU;      // its up
    vec4 LN;      // its normal, toward the camera
    vec4 Light0;  // grow, ember, logo, sun
    vec4 Light1;  // glow, boost, gather, seconds since ignition (-1 before)
    vec4 Light2;  // flash, sheet lightning, rim, beat
    vec4 Cur;     // the cursor's light in the world, strength
    vec4 Size;    // scene width, height; picture width, height
    vec4 Comp0;   // squeeze, bloom, trail, menu shade
    vec4 Comp1;   // corner shade, fade, picture px per ui px, rays
    vec4 Comp2;   // streak, -, light on screen (uv, y up)
    vec4 Comp3;   // the light's radius (of the height), -, -, -
    vec4 Lk;      // the title: x, y from the bottom, width, height (picture px)
    vec4 LkA;     // seconds of forging, lit, «cursed presents», shown
    vec4 LkM;     // top of the letters, their base, the rule (mask v)
    vec4 Hov0;    // a burning line under a word: x0, y (from the bottom), x1, progress (picture px)
    vec4 Hov1;
    vec4 HovA;    // their strength
    vec4 Parts;   // embers, set, seconds of forging, forge sparks
    vec4 Parts2;  // -, picture height, -, the rule's half width (of the title)
    vec4 Sim;     // dt, time, hold, seconds since the burst (-1 none)
    vec4 Sim2;    // aspect, init, alpha, point px
    vec4 Rule;    // the rule's height over the title's bottom (of its height), -, -, -
    // what the menus' screens ask of the world (the preview's knobs, menus/): a new world's terrain, the settings
    vec4 Knob0;   // the terrain: amplitude, flatness, its features' scale, its ceiling (blocks)
    vec4 Knob1;   // the seed's shift (x, z), the height past the map's edge, the ceiling the rays look under
    vec4 Knob2;   // the fog (the render distance), how far the ridges are drawn, clouds, how far they reach
    vec4 Knob3;   // reflections (0 off), the ridges' height, sparks' density, exposure
    vec4 Knob4;   // hardcore's red, dimming, the background's blur, the right side's shade
    vec4 Knob5;   // how much of the title in the corner is seen (the main menu's; the other screens let it go), the corners'
                  // shade (under the main menu's music and account), -, -
    // v12 («one line»): the world's wave of chunks, the light over it, the title, the world's frame (timeline.rs Frame)
    vec4 W0;      // the wave: its seconds, which way (1 loading from the horizon, -1 unloading from the camera, 0 all there); the world's own light; the scene on
    vec4 W1;      // the ridges risen: near, middle, far; -
    vec4 Tip;     // the tip (the spark, the wings' point, the horizon's line; ui px, y up); the view (ui px)
    vec4 LogoF;   // the wings: size (ui px), strength, seconds of their strokes, spread 0..1
    vec4 HL;      // the horizon's line: its length left, right of the tip (ui px), strength, «as the glow» 0..1
    vec4 FxA;     // the ember in the tip, scene px per ui px, -, -
    vec4 RuleF;   // the line under the word, its light: x0, x1, y (ui px, y up), how much is drawn
    vec4 RuleC;   // the same, crisp in the picture
    vec4 PointF;  // the burning point: x, strength, its trail (ui px), the line's heat behind it
    vec4 Spk;     // its sparks: x, y (ui px, y up), strength, speed (ui px a second)
    vec4 Title;   // the title: left, its line (ui px, y up), ui px an atlas px, shown
    vec4 Title2;  // the cells' pitch, the letters' rise over the line, the capital's height (ui px); the atlas's cells
    vec4 Lt[6];   // a letter: the old gone 0..1, the new risen 0..1, the new's heat, the old's heat
    vec4 WorldA;  // the world's frame: shown, seconds of its blocks, which way (1 put together, -1 crumbling), grey
    vec4 WorldB;  // the line its blocks come from and go to (ui px, y up), -, -, -
    vec4 WorldUV; // how the frame lies on the picture: its uv's scale and shift
};
#define uRiseT W0.x
#define uRiseDir W0.y
#define uLit W0.z
#define uAmp Knob0.x
#define uFlat Knob0.y
#define uScale Knob0.z
#define uTop Knob0.w
#define uSeed Knob1.xy
#define uOutH Knob1.z
#define uTopH Knob1.w
#define uFog Knob2.x
#define uFarZ Knob2.y
#define uClouds Knob2.z
#define uCloudFar Knob2.w
#define uRefl Knob3.x
#define uRidgeK Knob3.y
#define uDensity Knob3.z
#define uExpo Knob3.w
#define uHard Knob4.x
#define uDim Knob4.y
#define uBlurK Knob4.z
#define uRightShade Knob4.w

#define uTime Upv.w
#define uCam Cam.xyz
#define uTanH Cam.w
#define uRight Right.xyz
#define uAspect Right.w
#define uUp Upv.xyz
#define uFwd Fwd.xyz
#define uZO Fwd.w
#define uLC LC.xyz
#define uLS LC.w
#define uLR LR.xyz
#define uLU LU.xyz
#define uLN LN.xyz
#define uGrow Light0.x
#define uEmber Light0.y
#define uLogoI Light0.z
#define uSun Light0.w
#define uGlow Light1.x
#define uBoost Light1.y
#define uGather Light1.z
#define uIgn Light1.w
#define uFlash Light2.x
#define uFlashC Light2.y
#define uRim Light2.z
#define uBeat Light2.w
#define uCur Cur

float hash12(vec2 p){ vec3 p3 = fract(vec3(p.xyx) * .1031); p3 += dot(p3, p3.yzx + 33.33); return fract((p3.x + p3.y) * p3.z); }
float hash13(vec3 p3){ p3 = fract(p3 * .1031); p3 += dot(p3, p3.zyx + 31.32); return fract((p3.x + p3.y) * p3.z); }
float vn2(vec2 p){ vec2 i = floor(p), f = fract(p); vec2 u = f*f*(3.-2.*f);
  return mix(mix(hash12(i), hash12(i+vec2(1,0)), u.x), mix(hash12(i+vec2(0,1)), hash12(i+vec2(1,1)), u.x), u.y); }
float fbm2(vec2 p){ float s = 0., a = .5; for (int i = 0; i < 4; i++) { s += a * vn2(p); p = p * 2.03 + vec2(17.1, 9.3); a *= .5; } return s / .9375; }
vec2 gr2(vec2 i){ float h = hash12(i) * 6.2831853; return vec2(cos(h), sin(h)); }
float gn2(vec2 p){ vec2 i = floor(p), f = fract(p); vec2 u = f * f * f * (f * (f * 6. - 15.) + 10.);
  return mix(mix(dot(gr2(i), f), dot(gr2(i + vec2(1, 0)), f - vec2(1, 0)), u.x), mix(dot(gr2(i + vec2(0, 1)), f - vec2(0, 1)), dot(gr2(i + vec2(1, 1)), f - vec2(1, 1)), u.x), u.y); }
float hq(vec2 p, float s){ uvec2 q = uvec2(ivec2(p)) * uvec2(1597334673U, 3812015801U); uint n = (q.x ^ q.y ^ uint(s)) * 1597334673U; n ^= n >> 15; n *= 2246822519U; n ^= n >> 13; return float(n) * (1. / 4294967295.); }
