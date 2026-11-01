// Shared head of the window's own title bar (native/src/chrome): its numbers, and a little noise.
// The bar is drawn top down, px from its top-left corner, at the window's DPI.

layout(std140, set = 0, binding = 0) uniform Chrome {
    vec4 Size;    // the bar's width, height (px); px per 96-DPI px; seconds
    vec4 State;   // active (0..1), maximized, glyphs, a caption button's width (px)
    vec4 Hover;   // minimise, maximise, close: how lit by the cursor (0..1); -
    vec4 Press;   // minimise, maximise, close: how pressed (0..1); -
    vec4 Lines;   // the line under minimise, maximise, close: drawn in from the left (0..1), gone to the right (1..2); -
    vec4 Icon;    // her icon: x, y (top-left), size (px), alpha
    vec4 Field;   // the text's atlas: spread (atlas px), width, height; -
    vec4 Pill;    // how far the bar has come down into its place (0..1, its buttons with it); the window before the
                  // game's lone minimise: how far in (0..1), how lit; -
};

float hash12(vec2 p){ vec3 p3 = fract(vec3(p.xyx) * .1031); p3 += dot(p3, p3.yzx + 33.33); return fract((p3.x + p3.y) * p3.z); }
float vn2(vec2 p){ vec2 i = floor(p), f = fract(p); vec2 u = f*f*(3.-2.*f);
  return mix(mix(hash12(i), hash12(i+vec2(1,0)), u.x), mix(hash12(i+vec2(0,1)), hash12(i+vec2(1,1)), u.x), u.y); }
float fbm2(vec2 p){ float s = 0., a = .5; for (int i = 0; i < 4; i++) { s += a * vn2(p); p = p * 2.03 + vec2(17.1, 9.3); a *= .5; } return s / .9375; }
