// The voxel window's helpers and the shadow ray, shared by the terrain fragment shader and the shadow mask compute
// shader (engine/mod.rs, vk/shaders.rs put this in front of them). They need CameraBlockPos, CursedWindow, CursedSize,
// CursedVoxels and CursedBricks declared before.

// Voxel classes (engine/voxels.rs).
const uint C_FULL = 1u;
const uint C_SLAB_LOW = 2u;
const uint C_SLAB_TOP = 3u;
const uint C_LEAVES = 4u;
const uint C_GLASS = 5u;
const uint C_WATER = 6u;
const uint C_PLANT = 7u;
const uint C_PARTIAL = 8u;
const uint C_THIN = 9u;
const uint C_SWAY = 10u;
const uint C_SHINY = 11u;

bool cursed_inside(ivec3 w) {
    ivec3 r = w - CursedWindow.xyz;
    return r.x >= 0 && r.y >= 0 && r.z >= 0 && r.x < CursedWindow.w && r.z < CursedWindow.w && r.y < CursedSize.y;
}

uint cursed_voxel(ivec3 w) {
    return texelFetch(CursedVoxels, ivec3(w.x & 255, w.z & 255, w.y - CursedWindow.y), 0).r;
}

// Whether the block at `w` is thin enough for the sun to shine through to its far side (leaves, plants).
bool cursed_lets_through(ivec3 w) {
    if (!cursed_inside(w)) {
        return false;
    }
    uint c = cursed_voxel(w) & 15u;
    return c == C_LEAVES || c == C_PLANT || c == C_SWAY;
}

uint cursed_brick(ivec3 w) {
    return texelFetch(CursedBricks, ivec3((w.x & 255) >> 2, (w.z & 255) >> 2, (w.y - CursedWindow.y) >> 2), 0).r;
}

float cursed_hash(ivec3 c) {
    uint h = uint(c.x) * 73856093u ^ uint(c.y) * 19349663u ^ uint(c.z) * 83492791u;
    h ^= h >> 13;
    h *= 0x5bd1e995u;
    h ^= h >> 15;
    return float(h & 0xffffu) / 65535.0;
}

// The sun is a disc, not a point: a direction toward a random spot of it (radius about 1.7 degrees, wider than the
// real sun so the softening shows), different each frame for each pixel — the frames put together make the penumbra:
// sharp where a shadow begins, soft far from what casts it. Only when the frames are put together (flag 64).
vec3 cursed_soft(vec3 L, uvec3 seed) {
    if ((CursedSize.z & 64) == 0) {
        return L;
    }
    uint h = seed.x * 1973u + seed.y * 9277u + seed.z * 26699u;
    h = (h ^ 61u) ^ (h >> 16);
    h *= 9u;
    h ^= h >> 4;
    h *= 0x27d4eb2du;
    h ^= h >> 15;
    float a = float(h & 0xFFFFu) / 65535.0 * 6.2831853;
    float r = sqrt(float(h >> 16) / 65535.0) * 0.03;
    vec3 t = normalize(abs(L.y) < 0.9 ? cross(L, vec3(0.0, 1.0, 0.0)) : cross(L, vec3(1.0, 0.0, 0.0)));
    vec3 b = cross(L, t);
    return normalize(L + (t * cos(a) + b * sin(a)) * r);
}

#ifdef CURSED_CLOUD_SHADOWS
// The clouds' shadow at `o` (blocks from CameraBlockPos) for light from `L`: where the ray toward the light crosses
// the clouds' layer, the game's cloud map (CursedPalette rows 2..257, a cell 12 blocks) moved as the game moves it
// (CursedTime.w along x), softened over a few blocks. 1 without clouds (CursedSpare.w 0) and above them.
float cursed_cloud_shadow(vec3 o, vec3 L) {
    if (CursedSpare.w < 0.5 || L.y < 0.05) {
        return 1.0;
    }
    float t = (CursedSpare.z + 2.0 - o.y) / L.y;
    if (t <= 0.0) {
        return 1.0;
    }
    vec2 q = o.xz + L.xz * t;
    // The camera's block wrapped to the map's 3072 blocks (any whole number of turns does).
    ivec2 b = CameraBlockPos.xz - 3072 * ivec2(floor(vec2(CameraBlockPos.xz) / 3072.0));
    vec2 c = (vec2(b) + q + vec2(CursedTime.w, 3.96)) / 12.0 - 0.5;
    ivec2 i = ivec2(floor(c));
    vec2 f = c - vec2(i);
    float c00 = texelFetch(CursedPalette, ivec2(i.x & 255, 2 + (i.y & 255)), 0).r;
    float c10 = texelFetch(CursedPalette, ivec2((i.x + 1) & 255, 2 + (i.y & 255)), 0).r;
    float c01 = texelFetch(CursedPalette, ivec2(i.x & 255, 2 + ((i.y + 1) & 255)), 0).r;
    float c11 = texelFetch(CursedPalette, ivec2((i.x + 1) & 255, 2 + ((i.y + 1) & 255)), 0).r;
    float cover = smoothstep(0.2, 0.8, mix(mix(c00, c10, f.x), mix(c01, c11, f.x), f.y));
    return 1.0 - 0.6 * cover;
}
#endif

#ifdef CURSED_ENTITY_SHADOWS
// How much of the light from `o` (blocks from CameraBlockPos) along `d` gets past the creatures and players near the
// camera (CursedEntities: centre and half size, each an ellipsoid in its box): soft at the edges.
float cursed_entity_shadow(vec3 o, vec3 d) {
    float through = 1.0;
    int count = int(CursedSpare.y);
    for (int i = 0; i < count; i++) {
        vec3 size = CursedEntities[i * 2 + 1].xyz;
        vec3 oc = (o - CursedEntities[i * 2].xyz) / size;
        vec3 dc = d / size;
        float t = max(-dot(oc, dc) / dot(dc, dc), 0.0);
        through *= mix(0.1, 1.0, smoothstep(0.55, 1.05, length(oc + dc * t)));
    }
    return through;
}
#endif

// How much light gets from `o` (blocks from CameraBlockPos) along `d` out of the window, through at most `maxT`
// blocks: 1 open, 0 stopped, between for leaves, glass, water, fences. The cell `o` starts in is the surface's own.
float cursed_trace(vec3 o, vec3 d, float maxT) {
    ivec3 base = CameraBlockPos;
    ivec3 cell = ivec3(floor(o));
    ivec3 stp = ivec3(d.x > 0.0 ? 1 : -1, d.y > 0.0 ? 1 : -1, d.z > 0.0 ? 1 : -1);
    vec3 dd = vec3(abs(d.x) < 1e-6 ? 1e-6 : d.x, abs(d.y) < 1e-6 ? 1e-6 : d.y, abs(d.z) < 1e-6 ? 1e-6 : d.z);
    vec3 inv = 1.0 / dd;
    vec3 tDelta = abs(inv);
    vec3 far01 = vec3(stp.x > 0 ? 1.0 : 0.0, stp.y > 0 ? 1.0 : 0.0, stp.z > 0 ? 1.0 : 0.0);
    vec3 tMax = (vec3(cell) + far01 - o) * inv;
    float t = 0.0;
    float trans = 1.0;
    bool first = true;
    for (int i = 0; i < 128; i++) {
        if (!first) {
            ivec3 w = cell + base;
            if (t > maxT || !cursed_inside(w)) {
                break;
            }
            if (dd.y > 0.0) {
                // Above everything in the window, going up: nothing more to meet.
                if (w.y >= CursedSize.w) {
                    break;
                }
                // Above everything in this column of 16x16 blocks: on to where the ray leaves it.
                int c = ((w.x & 255) >> 4) + ((w.z & 255) >> 4) * 16;
                if (w.y >= CursedColumns[c >> 2][c & 3]) {
                    vec2 cmin = vec2(((w.x >> 4) << 4) - base.x, ((w.z >> 4) << 4) - base.z);
                    vec2 tFar = (cmin + far01.xz * 16.0 - o.xz) * inv.xz;
                    float tExit = min(tFar.x, tFar.y);
                    vec3 q = o + dd * (tExit + 1e-3);
                    cell = ivec3(floor(q));
                    tMax = (vec3(cell) + far01 - o) * inv;
                    t = tExit;
                    continue;
                }
            }
            if (cursed_brick(w) == 0u) {
                // Nothing in this brick: on to where the ray leaves it.
                vec3 bmin = vec3(((w >> 2) << 2) - base);
                vec3 tFar = (bmin + far01 * 4.0 - o) * inv;
                float tExit = min(min(tFar.x, tFar.y), tFar.z);
                vec3 q = o + dd * (tExit + 1e-3);
                cell = ivec3(floor(q));
                tMax = (vec3(cell) + far01 - o) * inv;
                t = tExit;
                continue;
            }
            uint c = cursed_voxel(w) & 15u;
            if (c == C_FULL || c == C_PARTIAL || c == C_SHINY) {
                return 0.0;
            } else if (c == C_SLAB_LOW || c == C_SLAB_TOP) {
                float tOut = min(min(tMax.x, tMax.y), tMax.z);
                float y0 = o.y + dd.y * t - float(cell.y);
                float y1 = o.y + dd.y * tOut - float(cell.y);
                if (c == C_SLAB_LOW ? min(y0, y1) < 0.5 : max(y0, y1) > 0.5) {
                    return 0.0;
                }
            } else if (c == C_LEAVES) {
                trans *= mix(0.25, 0.6, cursed_hash(w));
            } else if (c == C_GLASS) {
                trans *= 0.85;
            } else if (c == C_WATER) {
                trans *= 0.78;
            } else if (c == C_THIN) {
                trans *= 0.6;
            }
            if (trans < 0.12) {
                return 0.0;
            }
        }
        first = false;
        if (tMax.x < tMax.y && tMax.x < tMax.z) {
            t = tMax.x;
            tMax.x += tDelta.x;
            cell.x += stp.x;
        } else if (tMax.y < tMax.z) {
            t = tMax.y;
            tMax.y += tDelta.y;
            cell.y += stp.y;
        } else {
            t = tMax.z;
            tMax.z += tDelta.z;
            cell.z += stp.z;
        }
    }
    return trans;
}

