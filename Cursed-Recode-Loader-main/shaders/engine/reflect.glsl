// Reflections for water and glass (terrain.fsh, translucent): the first block a reflected ray meets in the voxel
// window, lit roughly by the sky over it, or the sky itself with the sun's glint. engine/mod.rs puts this after
// trace.glsl in front of the terrain fragment shader.

// The first block a ray from `o` (blocks from CameraBlockPos) along `d` meets within `maxT`: where (t), its voxel, the
// voxel of the cell the ray came from, and the face it hit. The start cell is the surface's own.
bool cursed_hit(vec3 o, vec3 d, float maxT, out float tHit, out uint hit, out uint before, out vec3 face) {
    ivec3 base = CameraBlockPos;
    ivec3 cell = ivec3(floor(o));
    ivec3 stp = ivec3(d.x > 0.0 ? 1 : -1, d.y > 0.0 ? 1 : -1, d.z > 0.0 ? 1 : -1);
    vec3 dd = vec3(abs(d.x) < 1e-6 ? 1e-6 : d.x, abs(d.y) < 1e-6 ? 1e-6 : d.y, abs(d.z) < 1e-6 ? 1e-6 : d.z);
    vec3 inv = 1.0 / dd;
    vec3 tDelta = abs(inv);
    vec3 far01 = vec3(stp.x > 0 ? 1.0 : 0.0, stp.y > 0 ? 1.0 : 0.0, stp.z > 0 ? 1.0 : 0.0);
    vec3 tMax = (vec3(cell) + far01 - o) * inv;
    float t = 0.0;
    int axis = 1;
    before = 0xF0u;
    tHit = 0.0;
    hit = 0u;
    face = vec3(0.0, 1.0, 0.0);
    bool first = true;
    for (int i = 0; i < 96; i++) {
        if (!first) {
            ivec3 w = cell + base;
            if (t > maxT || !cursed_inside(w)) {
                return false;
            }
            if (dd.y > 0.0 && w.y >= CursedSize.w) {
                return false;
            }
            if (cursed_brick(w) == 0u) {
                vec3 bmin = vec3(((w >> 2) << 2) - base);
                vec3 tFar = (bmin + far01 * 4.0 - o) * inv;
                float tExit = min(min(tFar.x, tFar.y), tFar.z);
                axis = tFar.x <= tFar.y && tFar.x <= tFar.z ? 0 : (tFar.y <= tFar.z ? 1 : 2);
                vec3 q = o + dd * (tExit + 1e-3);
                cell = ivec3(floor(q));
                tMax = (vec3(cell) + far01 - o) * inv;
                t = tExit;
                continue;
            }
            uint v = cursed_voxel(w);
            uint c = v & 15u;
            bool solid = c == C_FULL || c == C_PARTIAL || c == C_LEAVES || c == C_GLASS || c == C_SHINY;
            if (c == C_SLAB_LOW || c == C_SLAB_TOP) {
                float tOut = min(min(tMax.x, tMax.y), tMax.z);
                float y0 = o.y + dd.y * t - float(cell.y);
                float y1 = o.y + dd.y * tOut - float(cell.y);
                solid = c == C_SLAB_LOW ? min(y0, y1) < 0.5 : max(y0, y1) > 0.5;
            }
            if (solid) {
                tHit = t;
                hit = v;
                face = vec3(0.0);
                face[axis] = -float(stp[axis]);
                return true;
            }
            before = v;
        }
        first = false;
        if (tMax.x < tMax.y && tMax.x < tMax.z) {
            t = tMax.x;
            tMax.x += tDelta.x;
            cell.x += stp.x;
            axis = 0;
        } else if (tMax.y < tMax.z) {
            t = tMax.y;
            tMax.y += tDelta.y;
            cell.y += stp.y;
            axis = 1;
        } else {
            t = tMax.z;
            tMax.z += tDelta.z;
            cell.z += stp.z;
            axis = 2;
        }
    }
    return false;
}

// The sky seen along `r`: the game's sky colour, lighter low down, and the sun's glint.
vec3 cursed_sky(vec3 r) {
    float day = max(CursedSun.w, 0.08);
    vec3 sky = CursedSky.rgb;
    vec3 horizon = mix(sky, vec3(0.78, 0.86, 1.0) * day, 0.6);
    vec3 c = mix(horizon, sky, smoothstep(0.0, 0.45, r.y));
    float glint = pow(max(dot(r, CursedSun.xyz), 0.0), 900.0) * 5.0 * CursedSun.w * (1.0 - CursedSky.a);
    return c + CursedSunColor.rgb * glint;
}

// What the picture (before water) shows along `r` from `pos` (round the camera): a walk in growing steps, each point
// projected onto the screen and compared with the depth there, the point where it went behind found more exactly by
// halving. `found` how much to trust it (0 where it left the screen or met nothing, fading toward the edges).
vec3 cursed_screen_reflection(vec3 pos, vec3 r, out float found) {
    found = 0.0;
    mat4 viewProj = ProjMat * ModelViewMat;
    // Clip space is linear along the ray: a start and a step instead of a matrix at every point.
    vec4 c0 = viewProj * vec4(pos, 1.0);
    vec4 cd = viewProj * vec4(r, 0.0);
    const int STEPS = 24;
    float t = 0.25;
    float before = 0.0;
    float stepLength = 0.3;
    for (int i = 0; i < STEPS; i++) {
        vec4 c = c0 + cd * t;
        if (c.w <= 0.05) {
            break;
        }
        vec2 uv = c.xy / c.w * 0.5 + 0.5;
        if (any(lessThan(uv, vec2(0.0))) || any(greaterThan(uv, vec2(1.0)))) {
            break;
        }
        float sceneZ = textureLod(CursedSceneDepth, uv, 0.0).r;
        if (sceneZ > 0.0) {
            // The scene's distance along the view (reversed depth: z = P32 / w - P22 rearranged).
            float sceneW = ProjMat[3][2] / (sceneZ + ProjMat[2][2]);
            if (c.w > sceneW + 0.02 && c.w < sceneW + max(0.5, stepLength * 2.0)) {
                float lo = before;
                float hi = t;
                for (int k = 0; k < 4; k++) {
                    float mid = 0.5 * (lo + hi);
                    vec4 cm = c0 + cd * mid;
                    float zm = textureLod(CursedSceneDepth, cm.xy / cm.w * 0.5 + 0.5, 0.0).r;
                    float wm = zm > 0.0 ? ProjMat[3][2] / (zm + ProjMat[2][2]) : 1e9;
                    if (cm.w > wm + 0.02) {
                        hi = mid;
                    } else {
                        lo = mid;
                    }
                }
                vec4 ch = c0 + cd * hi;
                uv = clamp(ch.xy / ch.w * 0.5 + 0.5, vec2(0.0), vec2(1.0));
                vec2 edge = min(uv, 1.0 - uv);
                found = smoothstep(0.0, 0.08, min(edge.x, edge.y)) * (1.0 - smoothstep(0.6, 1.0, float(i) / float(STEPS)));
                return textureLod(CursedScene, uv, 0.0).rgb;
            }
        }
        before = t;
        t += stepLength;
        stepLength *= 1.2;
    }
    return vec3(0.0);
}

// What a mirror at this pixel shows along `r` (from `o`, blocks from CameraBlockPos).
vec3 cursed_reflection(vec3 o, vec3 r) {
    float t;
    uint hit, before;
    vec3 face;
    if (!cursed_hit(o, r, 72.0, t, hit, before, face)) {
        return cursed_sky(r);
    }
    uint material = hit >> 8;
    vec3 albedo = pow(texelFetch(CursedPalette, ivec2(int(material), 0), 0).rgb, vec3(1.0 / 2.2));
    vec4 glow = texelFetch(CursedPalette, ivec2(int(material), 1), 0);
    float sky = float((before >> 4) & 15u) / 15.0;
    float day = max(CursedSun.w, 0.12) * (1.0 - 0.5 * CursedSky.a);
    // The game's own shading of faces: tops lit, sides less, bottoms least.
    float side = face.y > 0.5 ? 1.0 : (face.y < -0.5 ? 0.5 : 0.75);
    vec3 lit = albedo * (0.12 + 0.88 * sky * day) * side + pow(glow.rgb, vec3(1.0 / 2.2)) * glow.a;
    // Far reflections thin into the sky, as the world thins into fog.
    return mix(lit, cursed_sky(r), smoothstep(40.0, 72.0, t));
}
