#version 450
// CS2's sniper scope (its legacy_sniper_scope shader, as its compiled code reads — line for line, the names ours): the
// picture seen through a round lens as high as 0.85 of the screen (narrower as the aim wanders), bent toward its edge,
// its colours parted there, darker away from the middle; black round it; the cross of thin black lines through the
// middle, blurring as the inaccuracy grows (by up to a tenth of the screen's height), fainter then; with a gap round
// the middle when one is asked for. The lens and the lines sway with the scope's bob.

layout(set = 0, binding = 0) uniform Scope {
    vec2 size;        // the picture's size in pixels
    vec2 bob;         // the scope's bob (pixels)
    float inaccuracy; // the weapon's (CS's GetInaccuracy)
    uint lineWidth;   // the lines' width (pixels; 1)
    int gap;          // the gap round the middle (pixels; 0 none)
    float pad;
} s;

layout(set = 0, binding = 1) uniform sampler2D picture;

layout(location = 0) out vec4 outColor;

float toSegment(vec2 p, vec2 a, vec2 b) {
    vec2 ab = b - a;
    vec2 ap = p - a;
    float t = dot(ap, ab);
    if (t <= 0.0) {
        return length(ap);
    }
    float l = dot(ab, ab);
    if (l <= t) {
        return length(p - b);
    }
    return length(p - (a + ab * (t / l)));
}

void main() {
    vec2 frag = gl_FragCoord.xy;
    vec2 uv = frag / s.size;
    vec2 q = uv - vec2(0.5);
    q.x *= s.size.x / s.size.y;
    q -= s.bob / s.size.y * -1.2;
    vec2 dir = normalize(q);
    float r = length(q);
    float wander = clamp(s.inaccuracy * 2.0, 0.0, 1.0);
    float radius = mix(0.425, 0.38, wander);
    float lens = smoothstep(radius, radius - 0.03, r);
    // bent toward the rim, more so at its very edge
    vec2 bent = uv + dir * pow(r, 6.0);
    float rim = smoothstep(radius - 0.04, radius, r);
    bent = mix(bent, bent + dir * rim * rim * 0.05, vec2(rim));
    // the colours parted along the radius
    vec2 part = dir * pow(smoothstep(0.05, 1.0, r), 0.5) * 0.5;
    float red = texture(picture, bent - part * 0.015).r;
    float green = texture(picture, bent - part * 0.02).g;
    float blue = texture(picture, bent - part * 0.025).b;
    // the cross: the lines through the middle (it sways a little more as the aim wanders)
    vec2 middle = s.size * 0.5 + vec2(0.25) + s.bob * wander * -4.0;
    float across = toSegment(frag, middle - vec2(s.size.x, 0.0), middle + vec2(s.size.x, 0.0));
    float down = toSegment(frag, middle - vec2(0.0, s.size.y), middle + vec2(0.0, s.size.y));
    float half_ = max(0.5, float(s.lineWidth) * 0.5);
    float blur = half_ + max(0.0, s.size.y * 0.1 * wander) + 0.25;
    float lines = clamp(smoothstep(half_, blur, across) * smoothstep(half_, blur, down), 0.0, 1.0);
    if (s.gap > 0) {
        vec2 c = middle + vec2(0.25);
        if (s.lineWidth % 2u == 0u) {
            c -= vec2(0.5);
        }
        float g = float(s.gap);
        float inGap = smoothstep(g + 0.5, g, distance(frag, c));
        lines = mix(clamp(lines + inGap * 0.5, 0.0, 1.0) + inGap * 0.03, lines, smoothstep(0.0, 0.02, wander));
    }
    vec3 seen = vec3(red, green, blue) * smoothstep(0.55, 0.2, r);
    vec3 col = mix(seen, seen * lines, vec3(clamp(mix(1.0, 0.3, pow(wander, 0.3)), 0.0, 1.0)));
    outColor = vec4(col * lens, 1.0);
}
