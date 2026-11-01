#version 450
// The far terrain's six faces (vk/impostor.rs) over the frame. Each pixel's ray from this frame's camera picks the face
// it points into and reads the colour (filtered: the silhouettes against the sky stay soft) and the depth there; the
// depth becomes the distance along the ray, the point a distance from the face's own camera (a tenth of a block off
// at most), and that point's depth in this frame. Where the faces hold nothing the pixel is left alone.

layout(std140, binding = 0) uniform Impostor {
    mat4 inverseViewProj;  // camera-relative, this frame
    mat4 viewProj;
    mat4 faces[6];         // each face's projection · view
    vec4 origins[6];       // xyz: the face's camera from this frame's camera, w: the part of its image it is drawn in
    vec4 screen;           // xy: 1 / the target's size, z: the faces' size in texels
    vec4 misc;             // x: the faces' near plane, y: how much farther than seen (no fight with live terrain), z: the quads' depth, w: dev (impostor.rs CURSED_DEV_IMPTINT)
    vec4 corners[24];      // the quads (impostor.vert)
};
layout(binding = 1) uniform sampler2DArray colors;
layout(binding = 2) uniform sampler2DArray depths;

layout(location = 0) flat in int face;

layout(location = 0) out vec4 fragColor;
layout(depth_less) out float gl_FragDepth;

const vec3 AXES[6] = vec3[](vec3(1, 0, 0), vec3(-1, 0, 0), vec3(0, 1, 0), vec3(0, -1, 0), vec3(0, 0, 1), vec3(0, 0, -1));

void main() {
    vec2 ndc = gl_FragCoord.xy * screen.xy * 2.0 - 1.0;
    // two points of the pixel's ray (a tenth and four tenths of a block out): the game bobs the view in its projection
    // while walking, and the camera is not on the ray (impostor.rs ray)
    vec4 near = inverseViewProj * vec4(ndc, 0.5, 1.0);
    vec4 far = inverseViewProj * vec4(ndc, 0.125, 1.0);
    vec3 d = normalize(far.xyz / far.w - near.xyz / near.w);
    vec3 a = abs(d);
    int f = a.x >= a.y && a.x >= a.z ? (d.x > 0.0 ? 0 : 1) : (a.y >= a.z ? (d.y > 0.0 ? 2 : 3) : (d.z > 0.0 ? 4 : 5));
    if (misc.w > 2.5) {
        // dev: one face's image over the window
        vec3 at = vec3(gl_FragCoord.x * screen.x, 1.0 - gl_FragCoord.y * screen.y, misc.w - 3.0);
        gl_FragDepth = misc.z;
        fragColor = texture(depths, at).r > 0.0 ? vec4(texture(colors, at).rgb, 1.0) : vec4(1.0, 0.0, 1.0, 1.0);
        return;
    }
    // the quads reach past their faces' edges: each pixel is its own face's
    if (f != face) {
        discard;
    }
    vec4 c = faces[f] * vec4(d, 1.0);
    // a face drawn at a part of its image (moving fast): that part, never the texels past it
    float part = origins[f].w;
    vec2 uv = clamp((c.xy / c.w * 0.5 + 0.5) * part, vec2(0.5 / screen.z), vec2(part - 0.5 / screen.z));
    // what the faces cover, from their depth (the game's alpha means nothing there): the four texels around, as the
    // colour is filtered from them — the empty ones are black, so the filtered colour comes premultiplied by it
    vec4 g = textureGather(depths, vec3(uv, float(f)), 0);
    vec4 covered = vec4(greaterThan(g, vec4(0.0)));
    vec2 w = fract(uv * screen.z - 0.5);
    float coverage = mix(mix(covered.w, covered.z, w.x), mix(covered.x, covered.y, w.x), w.y);
    if (coverage < 0.004) {
        if (misc.w > 1.5) {
            gl_FragDepth = misc.z;
            fragColor = vec4(1.0, 0.0, 1.0, 1.0);
            return;
        }
        discard;
    }
    vec3 color = texture(colors, vec3(uv, float(f))).rgb;
    if (misc.w > 0.5) {
        color = mix(color, vec3(f == 0 || f == 1 ? 1.0 : 0.0, f == 2 || f == 3 ? 1.0 : 0.0, f >= 4 ? 1.0 : 0.0) * coverage, 0.45);
    }
    float z = max(max(g.x, g.y), max(g.z, g.w));
    // reversed and endless: depth = near / distance along the face's axis
    float along = z > 0.0 ? misc.x / z : 1.0e6;
    float r = along / max(dot(d, AXES[f]), 1.0e-3) * misc.y;
    vec4 p = viewProj * vec4(origins[f].xyz + d * r, 1.0);
    gl_FragDepth = misc.w > 1.5 ? misc.z : min(p.z / p.w, misc.z);
    fragColor = vec4(color, coverage);
}
