// The engine's terrain vertex shader: the game's core/terrain.vsh, except that light is not looked up here — the
// fragment shader lights each pixel (shadows need where it is and which way it faces). Java puts the version, the
// defines and the game's uniform blocks (with their bindings) in front of this (cursed.recode.engine.EngineShaders).

layout(location = 0) in vec3 Position;
layout(location = 1) in vec4 Color;
layout(location = 2) in vec2 UV0;
// TerrainCompact: the light as two bytes read signed; the low eight bits are the game's value
layout(location = 3) in ivec2 UV2;
#ifdef MULTIDRAW_TERRAIN
layout(location = 4) in ivec3 ChunkPosition;
layout(location = 5) in float ChunkVisibility;
#endif

layout(location = 0) out float sphericalVertexDistance;
layout(location = 1) out float cylindricalVertexDistance;
layout(location = 2) out vec4 vertexColor;
layout(location = 3) out vec2 texCoord0;
layout(location = 4) out float chunkVisibility;
layout(location = 5) out vec3 cursedPos;
layout(location = 6) out vec2 cursedLight;
// Merged faces (vk/terrain.rs merge_plane): where in the plane, how the sprite repeats (terrain.fsh cursed_sample).
layout(location = 7) out vec2 cursedTile;
layout(location = 8) flat out int cursedTiled;
// The depth pass and the lit pass must land on the same depth to the bit.
invariant gl_Position;

// Voxel classes (engine/voxels.rs) the wind looks at.
const uint V_LEAVES = 4u;
const uint V_WATER = 6u;
const uint V_PLANT = 7u;
const uint V_SWAY = 10u;

uint cursed_class(ivec3 w) {
    ivec3 r = w - CursedWindow.xyz;
    if (r.x < 0 || r.y < 0 || r.z < 0 || r.x >= CursedWindow.w || r.z >= CursedWindow.w || r.y >= CursedSize.y) {
        return 1u;
    }
    return texelFetch(CursedVoxels, ivec3(w.x & 255, w.z & 255, r.y), 0).r & 15u;
}

// How much the vertex at `local` in the section at `section` sways: the height of plant under it in blocks (a tall
// plant's top 2), -1 for leaves, 0 when a block it touches holds it — a plant's root stays in its ground, leaves on
// their logs. It depends only on where the vertex is, so blocks sharing a corner move it alike (no cracks) and the
// depth pass and the lit pass agree. A solid block stops the search at the first block it touches that holds.
float cursed_sway(ivec3 section, vec3 local) {
    vec3 r = round(local);
    vec3 edge = vec3(lessThan(abs(local - r), vec3(1.0 / 4096.0)));
    ivec3 hi = section + ivec3(mix(floor(local), r, edge));
    ivec3 lo = hi - ivec3(edge);
    bool leaves = false;
    bool plant = false;
    ivec3 root = lo;
    for (int y = lo.y; y <= hi.y; y++) {
        for (int z = lo.z; z <= hi.z; z++) {
            for (int x = lo.x; x <= hi.x; x++) {
                uint c = cursed_class(ivec3(x, y, z));
                if (c == V_SWAY) {
                    if (y == lo.y && !plant) {
                        plant = true;
                        root = ivec3(x, y, z);
                    }
                } else if (c == V_LEAVES) {
                    leaves = true;
                } else if (c != 0u && c != V_WATER && c != V_PLANT) {
                    return 0.0;
                }
            }
        }
    }
    if (!plant) {
        return leaves ? -1.0 : 0.0;
    }
    // A plant: the plant blocks stacked under the vertex.
    int stack = 1;
    for (int k = 1; k < 3 && cursed_class(root - ivec3(0, k, 0)) == V_SWAY; k++) {
        stack++;
    }
    return float(stack - 1) + local.y + float(section.y - lo.y);
}

// Where the wind takes a vertex at `world` (blocks, wrapped) swaying `weight` (cursed_sway): a slow gust travelling
// downwind, each plant bending at its own pace and leaves fluttering. Stronger in the rain.
vec3 cursed_wind(vec3 world, float weight) {
    float t = CursedTime.z;
    float strength = 1.0 + 1.2 * CursedSky.a;
    const vec2 dir = vec2(0.8829, 0.4695);
    vec3 down = vec3(dir.x, 0.0, dir.y);
    vec3 side = vec3(-dir.y, 0.0, dir.x);
    float along = dot(world.xz, dir);
    float gust = 0.55 + 0.45 * sin(t * 0.53 - along * 0.09) * sin(t * 0.21 - along * 0.043 + 1.7);
    float flutter = sin(t * 5.3 + dot(world, vec3(1.7, 2.3, 1.3))) * sin(t * 3.1 + dot(world, vec3(-2.1, 1.1, 2.9)));
    if (weight < 0.0) {
        // Leaves: they shiver in place, a little more in a gust.
        vec3 shiver = vec3(flutter, 0.5 * sin(t * 4.3 + dot(world, vec3(2.7, 1.9, -1.4))), sin(t * 4.7 + dot(world, vec3(-1.3, 2.2, 1.9))));
        return (shiver * 0.03 + down * 0.025 * gust) * strength;
    }
    float h = weight;
    float bend = (0.5 + 0.5 * sin(t * 1.7 - along * 0.4 + world.y * 0.3)) * gust;
    vec3 off = (down * bend * 0.08 + side * flutter * 0.025) * pow(h, 1.3) * strength;
    // The stem keeps its length: what leans goes down a little.
    off.y = -dot(off.xz, off.xz) / (2.0 * max(h, 0.5));
    return off;
}

void main() {
#ifdef CURSED_COMPACT
    // TerrainCompact: positions in 1/2048 of a block from -8, UVs in 1/32768 (UNORM16 reads value / 65535).
    vec3 position = Position * (65535.0 / 2048.0) - 8.0;
    vec2 uv0 = UV0 * (65535.0 / 32768.0);
#else
    vec3 position = Position;
    vec2 uv0 = UV0;
#endif
    vec3 pos = position + (ChunkPosition - CameraBlockPos) + CameraOffset;
#ifndef CURSED_TRANSLUCENT
    // Wind (flag 512), near the camera only: farther than 64 blocks it moves less than a pixel.
    float near = 1.0 - smoothstep(40.0, 64.0, length(pos));
    if ((CursedSize.z & 512) != 0 && near > 0.0) {
        float weight = cursed_sway(ChunkPosition, position);
        if (weight != 0.0) {
            pos += cursed_wind(vec3(ChunkPosition & 4095) + position, weight) * near;
        }
    }
#endif
    // Chunk Animator (flags 2048 rise, 4096 fall, 8192 grow): a section coming in, by how far in it is (the game's fade,
    // as long as the module says), easing as it settles
    int cursed_anim = CursedSize.z & (2048 | 4096 | 8192);
    if (cursed_anim != 0 && ChunkVisibility < 1.0) {
        float away = 1.0 - clamp(ChunkVisibility, 0.0, 1.0);
        away *= away;
        if ((cursed_anim & 2048) != 0) {
            pos.y -= away * 18.0;
        } else if ((cursed_anim & 4096) != 0) {
            pos.y += away * 26.0;
        } else {
            vec3 middle = vec3(ChunkPosition - CameraBlockPos) + CameraOffset + 8.0;
            pos = middle + (pos - middle) * (1.0 - away);
        }
    }
    gl_Position = ProjMat * ModelViewMat * vec4(pos, 1.0);

    sphericalVertexDistance = length(pos);
    cylindricalVertexDistance = max(length(pos.xz), abs(pos.y));
    vertexColor = Color;
    texCoord0 = uv0;
    cursedTile = position.zx;
    int cursed_code = int(Color.a * 255.0 + 0.5);
    cursedTiled = cursed_code < 128 ? 128 + cursed_code : 0;
    if (cursedTiled != 0) {
        vertexColor.a = 1.0;
    }
    cursedPos = pos;
    cursedLight = vec2(UV2 & 255);

    const float chunkFullyVisibleRange = 16.0;
    float dist = length(pos);
    chunkVisibility = mix(1.0, ChunkVisibility, clamp((dist - chunkFullyVisibleRange) / chunkFullyVisibleRange, 0.0, 1.0));
}
