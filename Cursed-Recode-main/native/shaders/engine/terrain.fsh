// The engine's terrain fragment shader: the game's core/terrain.fsh, lit here instead of in the vertex shader, with
// the sun's (or the moon's) shadow traced through the voxel window (engine/gpu.rs). Light is only ever taken away
// from what the game gives: where the game's light says dark, it stays dark. Java puts the version, the defines and
// the uniform blocks (the game's and CursedEngine, with their bindings) in front (cursed.recode.engine.EngineShaders).

layout(location = 0) in float sphericalVertexDistance;
layout(location = 1) in float cylindricalVertexDistance;
layout(location = 2) in vec4 vertexColor;
layout(location = 3) in vec2 texCoord0;
layout(location = 4) in float chunkVisibility;
layout(location = 5) in vec3 cursedPos;
layout(location = 6) in vec2 cursedLight;

layout(location = 0) out vec4 fragColor;

// ---- the game's helpers (include/fog.glsl, include/texture_sampling.glsl) ----

float linear_fog_value(float vertexDistance, float fogStart, float fogEnd) {
    if (vertexDistance <= fogStart) {
        return 0.0;
    } else if (vertexDistance >= fogEnd) {
        return 1.0;
    }
    return (vertexDistance - fogStart) / (fogEnd - fogStart);
}

float total_fog_value(float sphericalVertexDistance, float cylindricalVertexDistance, float environmentalStart, float environmantalEnd, float renderDistanceStart, float renderDistanceEnd) {
    return max(linear_fog_value(sphericalVertexDistance, environmentalStart, environmantalEnd), linear_fog_value(cylindricalVertexDistance, renderDistanceStart, renderDistanceEnd));
}

vec4 apply_fog(vec4 inColor, float sphericalVertexDistance, float cylindricalVertexDistance, float environmentalStart, float environmantalEnd, float renderDistanceStart, float renderDistanceEnd, vec4 fogColor) {
    float fogValue = total_fog_value(sphericalVertexDistance, cylindricalVertexDistance, environmentalStart, environmantalEnd, renderDistanceStart, renderDistanceEnd);
    return vec4(mix(inColor.rgb, fogColor.rgb, fogValue * fogColor.a), inColor.a);
}

vec4 sampleNearest(sampler2D source, vec2 uv, vec2 pixelSize, vec2 du, vec2 dv, vec2 texelScreenSize) {
    vec2 uvTexelCoords = uv / pixelSize;
    vec2 texelCenter = round(uvTexelCoords) - 0.5f;
    vec2 texelOffset = uvTexelCoords - texelCenter;
    texelOffset = (texelOffset - 0.5f) * pixelSize / texelScreenSize + 0.5f;
    texelOffset = clamp(texelOffset, 0.0f, 1.0f);
    uv = (texelCenter + texelOffset) * pixelSize;
    return textureGrad(source, uv, du, dv);
}

vec4 sampleNearest(sampler2D source, vec2 uv, vec2 pixelSize) {
    // Drawn smaller than the window (engine/upscale.rs): the texture's detail as for the window's pixels.
    vec2 du = dFdx(uv) * CursedTime.y;
    vec2 dv = dFdy(uv) * CursedTime.y;
    vec2 texelScreenSize = sqrt(du * du + dv * dv);
    return sampleNearest(source, uv, pixelSize, du, dv, texelScreenSize);
}

vec4 sampleRGSS(sampler2D source, vec2 uv, vec2 pixelSize) {
    vec2 du = dFdx(uv) * CursedTime.y;
    vec2 dv = dFdy(uv) * CursedTime.y;
    vec2 texelScreenSize = sqrt(du * du + dv * dv);
    float maxTexelSize = max(texelScreenSize.x, texelScreenSize.y);
    float minPixelSize = min(pixelSize.x, pixelSize.y);
    float transitionStart = minPixelSize * 1.0;
    float transitionEnd = minPixelSize * 2.0;
    float blendFactor = smoothstep(transitionStart, transitionEnd, maxTexelSize);
    float duLength = length(du);
    float dvLength = length(dv);
    float minDerivative = min(duLength, dvLength);
    float maxDerivative = max(duLength, dvLength);
    float effectiveDerivative = sqrt(minDerivative * maxDerivative);
    float mipLevelExact = max(0.0, log2(effectiveDerivative / minPixelSize));
    float mipLevelLow = floor(mipLevelExact);
    float mipLevelHigh = mipLevelLow + 1.0;
    float mipBlend = fract(mipLevelExact);
    const vec2 offsets[4] = vec2[](vec2(0.125, 0.375), vec2(-0.125, -0.375), vec2(0.375, -0.125), vec2(-0.375, 0.125));
    vec4 rgssColorLow = vec4(0.0);
    vec4 rgssColorHigh = vec4(0.0);
    for (int i = 0; i < 4; ++i) {
        vec2 sampleUV = uv + offsets[i] * pixelSize;
        rgssColorLow += textureLod(source, sampleUV, mipLevelLow);
        rgssColorHigh += textureLod(source, sampleUV, mipLevelHigh);
    }
    rgssColorLow *= 0.25;
    rgssColorHigh *= 0.25;
    vec4 rgssColor = mix(rgssColorLow, rgssColorHigh, mipBlend);
    vec4 nearestColor = sampleNearest(source, uv, pixelSize, du, dv, texelScreenSize);
    return mix(nearestColor, rgssColor, blendFactor);
}

vec3 cursed_lightmap(vec2 uv) {
    return textureLod(Sampler2, clamp(uv / 256.0 + 0.5 / 16.0, vec2(0.5 / 16.0), vec2(15.5 / 16.0)), 0.0).rgb;
}

// The coloured light grids (shaders/engine/light.comp) at the block in front of this surface: the glow's colour
// (strongest channel 1, times its level) and the light thrown back by surfaces in the open. Zero far from the camera.
void cursed_grids(vec3 pos, vec3 n, out vec3 glow, out vec3 bounce, out float near) {
    near = (1.0 - smoothstep(52.0, 62.0, max(abs(pos.x), abs(pos.z)))) * (1.0 - smoothstep(26.0, 31.0, abs(pos.y)));
    if ((CursedSize.z & 2) == 0 || near <= 0.0) {
        glow = vec3(0.0);
        bounce = vec3(0.0);
        near = 0.0;
        return;
    }
    // In blocks from CameraBlockPos, then the grid's texel (it wraps every 128 x 64 x 128 blocks).
    vec3 at = vec3(CameraBlockPos & ivec3(127, 63, 127)) + pos - CameraOffset + n * 0.5;
    vec3 uvw = at / vec3(128.0, 64.0, 128.0);
    glow = textureLod(CursedLight, uvw, 0.0).rgb;
    bounce = textureLod(CursedBounce, uvw, 0.0).rgb;
}

// One net of caustic lines at `p`: the edges between cells whose points drift in small circles (1 on a line).
float cursed_caustic_net(vec2 p, float t) {
    vec2 cell = floor(p);
    vec2 f = p - cell;
    float d1 = 8.0;
    float d2 = 8.0;
    for (int y = -1; y <= 1; y++) {
        for (int x = -1; x <= 1; x++) {
            ivec2 c = ivec2(cell) + ivec2(x, y);
            vec2 h = vec2(cursed_hash(ivec3(c, 7)), cursed_hash(ivec3(c, 13)));
            vec2 at = vec2(x, y) + 0.5 + 0.36 * sin(t * (0.5 + 0.6 * h.yx) + 6.2831853 * h);
            float d = length(at - f);
            if (d < d1) {
                d2 = d1;
                d1 = d;
            } else if (d < d2) {
                d2 = d;
            }
        }
    }
    return 1.0 - smoothstep(0.0, 0.18, d2 - d1);
}

// Sunlight through moving water onto what lies under it (flags 1 and 8): the waves gather it into bright lines and
// leave darker cells between, sharp under shallow water, fading out a few blocks down. 1 where there is no water above.
float cursed_caustics(vec3 pos, vec3 n, vec3 L) {
    if ((CursedSize.z & 8) == 0 || length(pos) > 48.0) {
        return 1.0;
    }
    ivec3 front = ivec3(floor(pos - CameraOffset + n * 0.5)) + CameraBlockPos;
    if (!cursed_inside(front) || (cursed_voxel(front) & 15u) != C_WATER) {
        return 1.0;
    }
    // How far down: the water above this point, up to 6 blocks.
    int depth = 1;
    while (depth < 6 && cursed_inside(front + ivec3(0, depth, 0)) && (cursed_voxel(front + ivec3(0, depth, 0)) & 15u) == C_WATER) {
        depth++;
    }
    vec3 world = vec3(CameraBlockPos & ivec3(4095)) + pos - CameraOffset;
    // Where the sun's ray through this point met the surface (bent toward straight down by the water).
    vec3 Lw = normalize(vec3(L.xz * 0.75, L.y).xzy);
    vec2 p = world.xz + Lw.xz / max(Lw.y, 0.3) * (float(depth) - fract(world.y));
    float t = CursedTime.z;
    float a = cursed_caustic_net(p * 0.85, t);
    float b = cursed_caustic_net(p * 1.27 + vec2(5.3, 1.7), t * 1.3);
    float lines = max(a, b) * 0.6 + a * b * 0.9;
    float fade = exp(-float(depth) * 0.3) * (1.0 - smoothstep(32.0, 48.0, length(pos)));
    return mix(1.0, 0.55 + 1.6 * lines, fade);
}

// The game's light at this pixel: the blocks' light coloured by what gives it, the sky's part shaded where the sun
// (or the moon) does not reach, and the light surfaces in the open throw back into the shade.
vec3 cursed_light(vec3 pos, vec2 light, vec3 n) {
    vec3 full = cursed_lightmap(light);
    vec3 block = cursed_lightmap(vec2(light.x, 0.0));
    vec3 sky = max(full - block, vec3(0.0));
    vec3 glow, bounce;
    float near;
    cursed_grids(pos, n, glow, bounce, near);
    float strongest = max(glow.r, max(glow.g, glow.b));
    if (strongest > 0.02 && light.x > 0.0) {
        // The game's brightness, the glow's hue (keeping its brightness to the eye).
        vec3 hue = glow / strongest;
        float lum = dot(hue, vec3(0.2126, 0.7152, 0.0722));
        hue = mix(vec3(lum), hue, 0.75) / max(lum, 0.4);
        block = mix(block, block * hue, 0.8 * near * smoothstep(0.0, 0.35, strongest));
    }
    if ((CursedSize.z & 1) == 0 || light.y < 8.0) {
        return block + sky + bounce * 0.35 * near;
    }
    vec3 L;
    float strength;
    vec3 tint;
    if (CursedSun.w > 0.02) {
        L = CursedSun.xyz;
        strength = CursedSun.w;
        tint = CursedSunColor.rgb;
    } else {
        L = CursedMoon.xyz;
        strength = CursedMoon.w;
        tint = vec3(0.8, 0.88, 1.0);
    }
    strength *= (1.0 - smoothstep(96.0, 124.0, max(abs(pos.x), abs(pos.z)))) * (1.0 - 0.85 * CursedSky.a) * (light.y / 240.0);
    if (strength < 0.01) {
        return block + sky + bounce * 0.35 * near;
    }
    float ndl = dot(n, L);
    // Leaves and plants are thin: the sun shines through them onto their far side, dimmer, brighter where the eye looks
    // toward the sun through them.
    float through = 1.0;
    if (ndl <= 0.0 && cursed_lets_through(ivec3(floor(pos - CameraOffset - n * 0.05)) + CameraBlockPos)) {
        through = 0.4 * (1.0 + 2.5 * pow(max(dot(normalize(pos), L), 0.0), 6.0));
        n = -n;
        ndl = -ndl;
    }
    float vis = 0.0;
    if (ndl > 0.0) {
        bool traced = false;
#ifdef CURSED_SHADE
        // Traced before this pass for one pixel of each 2x2 (shaders/engine/mask.comp): taken when that pixel is on
        // this surface (its depth within a fiftieth), else a neighbouring one on the side of this pixel, else traced.
        ivec2 h = ivec2(gl_FragCoord.xy) / 2;
        ivec2 side = ivec2(gl_FragCoord.xy) - h * 2;
        ivec2 nearby[4] = ivec2[](h, h + ivec2(side.x * 2 - 1, 0), h + ivec2(0, side.y * 2 - 1), h + side * 2 - 1);
        for (int k = 0; k < 4 && !traced; k++) {
            vec2 m = texelFetch(CursedMask, clamp(nearby[k], ivec2(0), textureSize(CursedMask, 0) - 1), 0).rg;
            if (abs(m.g - gl_FragCoord.z) <= gl_FragCoord.z * 0.02) {
                vis = m.r;
                traced = true;
            }
        }
#endif
        if (!traced) {
            float maxT = mix(160.0, 48.0, smoothstep(32.0, 112.0, length(cursedPos)));
            if (((CursedSize.z >> 16) & 255) > 0) {
                maxT = float((CursedSize.z >> 16) & 255);
            }
            vec3 o = cursedPos - CameraOffset + n * 0.02;
            vec3 soft = cursed_soft(L, uvec3(uvec2(gl_FragCoord.xy), uint(CursedSpare.x)));
            vis = cursed_trace(o, soft, maxT) * cursed_entity_shadow(o, soft);
        }
    }
    float direct = vis * smoothstep(0.0, 0.3, ndl) * through;
    // The clouds and the water above only change light that gets here at all.
    if (direct > 0.0) {
        direct *= cursed_cloud_shadow(pos - CameraOffset, L);
    }
    float shade = mix(1.0, mix(CursedSunColor.a, 1.0, direct), strength);
    vec3 color = mix(vec3(1.0), mix(vec3(0.86, 0.93, 1.1), tint, direct), strength);
#ifndef CURSED_TRANSLUCENT
    if (direct > 0.0) {
        shade *= mix(1.0, cursed_caustics(pos, n, L), direct * strength);
    }
#endif
    // What surfaces in the open throw back shows most where the sun itself does not reach.
    return block + sky * shade * color + bounce * near * mix(0.35, 0.12, direct);
}

// The slope of the water's surface here: the moving water (shaders/engine/water.comp, 4 cells a block, wrapping every
// 64 blocks) and a slow swell of a few long waves.
vec3 cursed_water_normal(vec3 pos) {
    vec2 at = vec2(CameraBlockPos.xz & ivec2(63)) + pos.xz - CameraOffset.xz;
    vec2 slope = vec2(0.0);
    if ((CursedSize.z & 8) != 0 && max(abs(pos.x), abs(pos.z)) < 30.0) {
        vec2 uv = at / 64.0;
        float e = 1.0 / 256.0;
        float hx = textureLod(CursedWater, uv + vec2(e, 0.0), 0.0).x - textureLod(CursedWater, uv - vec2(e, 0.0), 0.0).x;
        float hz = textureLod(CursedWater, uv + vec2(0.0, e), 0.0).x - textureLod(CursedWater, uv - vec2(0.0, e), 0.0).x;
        slope += vec2(hx, hz) * 1.4 * (1.0 - smoothstep(22.0, 30.0, max(abs(pos.x), abs(pos.z))));
    }
    float time = CursedTime.x * 0.05;
    vec2 world = vec2(CameraBlockPos.xz) + pos.xz - CameraOffset.xz;
    // Long, slow waves in three directions.
    const vec3 WAVES[3] = vec3[](vec3(0.8, 0.6, 0.9), vec3(-0.5, 0.85, 1.3), vec3(0.2, -1.0, 2.1));
    for (int i = 0; i < 3; i++) {
        vec2 dir = normalize(WAVES[i].xy);
        float k = WAVES[i].z;
        slope += dir * cos(dot(world, dir) * k + time * (1.5 + float(i))) * 0.022 / k;
    }
    return normalize(vec3(-slope.x, 1.0, -slope.y));
}

#ifndef CURSED_TRANSLUCENT
// Smooth noise over the ground, 0..1.
float cursed_noise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = p - i;
    f = f * f * (3.0 - 2.0 * f);
    ivec2 c = ivec2(i);
    float a = cursed_hash(ivec3(c, 31));
    float b = cursed_hash(ivec3(c + ivec2(1, 0), 31));
    float d = cursed_hash(ivec3(c + ivec2(0, 1), 31));
    float e = cursed_hash(ivec3(c + ivec2(1, 1), 31));
    return mix(mix(a, b, f.x), mix(d, e, f.x), f.y);
}

// Rain on what is open to the sky (flag 8): surfaces darken and gloss, flat ground gathers puddles that mirror the
// world, with rings spreading where drops fall. How wet: CursedExtra.x (it soaks up over the rain where it rains and
// dries after). `light` the game's light coordinates, `n` the face.
vec3 cursed_wet(vec3 color, vec3 pos, vec2 light, vec3 n) {
    float rain = CursedExtra.x;
    float far = length(pos);
    if ((CursedSize.z & 8) == 0 || rain < 0.01 || far > 48.0) {
        return color;
    }
    float wet = rain * smoothstep(200.0, 236.0, light.y) * (1.0 - smoothstep(32.0, 48.0, far));
    if (wet < 0.01) {
        return color;
    }
    vec3 world = vec3(CameraBlockPos & ivec3(4095)) + pos - CameraOffset;
    vec3 V = pos / far;
    vec3 surface = n;
    float puddle = 0.0;
    if (n.y > 0.9) {
        // Puddles where the ground dips (a slow noise), more of them as the rain goes on; only on blocks that hold water.
        // Not on grass and moss (green): water soaks in there.
        uint v = cursed_voxel(ivec3(floor(pos - CameraOffset - n * 0.05)) + CameraBlockPos);
        uint under = v & 15u;
        vec3 albedo = texelFetch(CursedPalette, ivec2(int(v >> 8), 0), 0).rgb;
        bool green = albedo.g > albedo.r * 1.1 && albedo.g > albedo.b * 1.1;
        if (!green && (under == C_FULL || under == C_SLAB_LOW || under == C_SLAB_TOP || under == C_PARTIAL || under == C_SHINY)) {
            float ground = cursed_noise(world.xz * 0.21) * 0.65 + cursed_noise(world.xz * 0.63 + 3.1) * 0.35;
            puddle = smoothstep(0.6 - 0.1 * rain, 0.66 - 0.1 * rain, ground) * wet;
        }
        if (puddle > 0.0) {
            // A drop in each cell of a third of a block, each at its own time: a ring spreading and fading.
            vec2 q = world.xz * 3.0;
            vec2 cell = floor(q);
            float h = cursed_hash(ivec3(ivec2(cell), 53));
            float phase = fract(CursedTime.z * 0.9 + h);
            vec2 d = q - (cell + 0.5 + (vec2(h, fract(h * 7.31)) - 0.5) * 0.1);
            float r = length(d);
            float ring = exp(-pow((r - phase * 0.45) * 18.0, 2.0)) * (1.0 - phase);
            vec2 slope = d / max(r, 1e-3) * ring * 0.35 * CursedSky.a;
            surface = normalize(vec3(-slope.x, 1.0, -slope.y));
        }
    }
    // Wet: darker, and a sheen of the sky; a puddle is a mirror over darker ground (a little of it even seen from
    // above, where the sky is far brighter than the wet ground).
    color *= mix(1.0, 0.8, wet) * mix(1.0, 0.6, puddle);
    vec3 R = reflect(V, surface);
    float F = 0.02 + 0.98 * pow(1.0 - clamp(dot(-V, surface), 0.0, 1.0), 5.0);
    vec3 mirrored = puddle > 0.0 ? cursed_reflection(pos - CameraOffset + n * 0.05, R) : cursed_sky(R);
    return mix(color, mirrored, clamp(mix(F * 0.35 * wet, 0.12 + 0.88 * F, puddle), 0.0, 1.0));
}

// Metal blocks (gold, iron, diamond, emerald, netherite, copper; flag 4): they mirror the world tinted by their own
// colour, more at a slant, and the sun glints off them where it reaches. `albedo` the texture's colour, `lit` the light,
// `own` the voxel of the block this pixel belongs to.
vec3 cursed_metal(vec3 color, vec3 albedo, vec3 lit, vec3 pos, vec3 n, uint own) {
    float far = length(pos);
    if ((CursedSize.z & 4) == 0 || far > 64.0 || (own & 15u) != C_SHINY) {
        return color;
    }
    vec3 V = pos / far;
    vec3 R = reflect(V, n);
    float F = 0.35 + 0.65 * pow(1.0 - clamp(dot(-V, n), 0.0, 1.0), 5.0);
    vec3 mirrored = cursed_reflection(pos - CameraOffset + n * 0.05, R);
    // How lit the block is against full daylight: no glint in the shade.
    float sunlit = smoothstep(0.75, 1.05, max(lit.r, max(lit.g, lit.b)));
    float glint = pow(max(dot(R, CursedSun.xyz), 0.0), 120.0) * 3.0 * CursedSun.w * (1.0 - CursedSky.a) * sunlit;
    vec3 shine = mirrored * albedo * 1.6 + CursedSunColor.rgb * albedo * glint;
    // On top of the light it has (a sunlit top stays bright), what it mirrors more at a slant.
    float k = F * (1.0 - smoothstep(48.0, 64.0, far));
    return color * (1.0 - 0.45 * k) + shine * k;
}
#endif

#ifdef CURSED_TRANSLUCENT
// Foam where the water's surface meets the shore (flag 8): along the sides and corners of its block that touch
// something solid, broken up by a slow drifting noise. 0 none, 1 thick.
float cursed_foam(vec3 pos) {
    if ((CursedSize.z & 8) == 0 || length(pos) > 48.0) {
        return 0.0;
    }
    vec3 o = pos - CameraOffset;
    ivec3 cell = ivec3(floor(o - vec3(0.0, 0.05, 0.0))) + CameraBlockPos;
    vec2 f = o.xz - floor(o.xz);
    float d = 1.0;
    // Only the neighbours that could be near enough (foam reaches 0.6 of a block at most).
    ivec2 from = ivec2(f.x < 0.6 ? -1 : 0, f.y < 0.6 ? -1 : 0);
    ivec2 to = ivec2(f.x > 0.4 ? 1 : 0, f.y > 0.4 ? 1 : 0);
    for (int dz = from.y; dz <= to.y; dz++) {
        for (int dx = from.x; dx <= to.x; dx++) {
            if (dx == 0 && dz == 0) {
                continue;
            }
            ivec3 w = cell + ivec3(dx, 0, dz);
            if (!cursed_inside(w)) {
                continue;
            }
            uint c = cursed_voxel(w) & 15u;
            if (c == C_FULL || c == C_SLAB_LOW || c == C_SLAB_TOP || c == C_PARTIAL || c == C_SHINY) {
                vec2 lo = vec2(dx, dz);
                vec2 gap = max(max(lo - f, f - (lo + 1.0)), vec2(0.0));
                d = min(d, length(gap));
            }
        }
    }
    if (d >= 0.6) {
        return 0.0;
    }
    // A value noise drifting along the shore, two sizes: foam in patches and threads, not a glow.
    vec2 world = vec2(CameraBlockPos.xz & ivec2(1023)) + o.xz;
    float t = CursedTime.z;
    float n = 0.0;
    for (int k = 0; k < 2; k++) {
        vec2 p = world * (k == 0 ? 2.7 : 6.1) + vec2(t * 0.21, -t * 0.17) * (k == 0 ? 1.0 : -1.4);
        vec2 i = floor(p);
        vec2 u = p - i;
        u = u * u * (3.0 - 2.0 * u);
        ivec2 c = ivec2(i);
        float v = mix(mix(cursed_hash(ivec3(c, 71 + k)), cursed_hash(ivec3(c + ivec2(1, 0), 71 + k)), u.x),
                      mix(cursed_hash(ivec3(c + ivec2(0, 1), 71 + k)), cursed_hash(ivec3(c + ivec2(1, 1), 71 + k)), u.x), u.y);
        n += v * (k == 0 ? 0.65 : 0.35);
    }
    // The edge breathes in and out a little, like a wash; thick at the shore, in bits farther out.
    float edge = 0.4 + 0.12 * sin(t * 0.9 + dot(world, vec2(0.7, 0.4)));
    float near = 1.0 - smoothstep(0.0, edge, d);
    return smoothstep(0.42, 0.5, n * 0.7 + near * 0.55) * (1.0 - smoothstep(edge * 0.8, edge, d));
}
#endif

// A rectangle of merged faces (vk/terrain.rs merge_plane) repeats its sprite once a block: cursedTiled is 128 + how
// (bit 0 u runs along the second in-plane axis, bits 1-2 u and v run backwards, bits 3-4 and 5-6 the sprite's size as
// 2^-(5 + n) of the atlas), 0 for the game's own faces; cursedTile is where in the plane, in blocks; the UV is the
// sprite's corner. The texture's derivatives come from the unrepeated coordinates, so mip levels match across blocks.
layout(location = 7) in vec2 cursedTile;
layout(location = 8) flat in int cursedTiled;

vec4 cursed_sampleRGSS(sampler2D source, vec2 uv, vec2 pixelSize, vec2 du, vec2 dv) {
    vec2 texelScreenSize = sqrt(du * du + dv * dv);
    float maxTexelSize = max(texelScreenSize.x, texelScreenSize.y);
    float minPixelSize = min(pixelSize.x, pixelSize.y);
    float blendFactor = smoothstep(minPixelSize * 1.0, minPixelSize * 2.0, maxTexelSize);
    float duLength = length(du);
    float dvLength = length(dv);
    float effectiveDerivative = sqrt(min(duLength, dvLength) * max(duLength, dvLength));
    float mipLevelExact = max(0.0, log2(effectiveDerivative / minPixelSize));
    float mipLevelLow = floor(mipLevelExact);
    float mipLevelHigh = mipLevelLow + 1.0;
    float mipBlend = fract(mipLevelExact);
    const vec2 offsets[4] = vec2[](vec2(0.125, 0.375), vec2(-0.125, -0.375), vec2(0.375, -0.125), vec2(-0.375, 0.125));
    vec4 rgssColorLow = vec4(0.0);
    vec4 rgssColorHigh = vec4(0.0);
    for (int i = 0; i < 4; ++i) {
        vec2 sampleUV = uv + offsets[i] * pixelSize;
        rgssColorLow += textureLod(source, sampleUV, mipLevelLow);
        rgssColorHigh += textureLod(source, sampleUV, mipLevelHigh);
    }
    vec4 rgssColor = mix(rgssColorLow * 0.25, rgssColorHigh * 0.25, mipBlend);
    vec4 nearestColor = sampleNearest(source, uv, pixelSize, du, dv, texelScreenSize);
    return mix(nearestColor, rgssColor, blendFactor);
}

vec4 cursed_sample(sampler2D source, vec2 uv, vec2 pixelSize) {
    if (cursedTiled == 0) {
        return UseRgss == 1 ? sampleRGSS(source, uv, pixelSize) : sampleNearest(source, uv, pixelSize);
    }
    int c = cursedTiled - 128;
    vec2 t = (c & 1) != 0 ? cursedTile.yx : cursedTile;
    vec2 size = vec2(1.0 / float(1 << (5 + ((c >> 3) & 3))), 1.0 / float(1 << (5 + ((c >> 5) & 3))));
    bvec2 back = bvec2((c & 2) != 0, (c & 4) != 0);
    vec2 f = fract(t);
    f = mix(f, 1.0 - f, back);
    vec2 along = mix(t, -t, back);
    vec2 du = dFdx(along * size);
    vec2 dv = dFdy(along * size);
    vec2 at = uv + f * size;
    return UseRgss == 1 ? cursed_sampleRGSS(source, at, pixelSize, du, dv) : sampleNearest(source, at, pixelSize, du, dv, sqrt(du * du + dv * dv));
}

#ifdef CURSED_SHADE
// The opaque terrain's depth is laid down first (CURSED_PREPASS) and this pass tests it for equality: every pixel is lit
// once, whatever lies behind it.
layout(early_fragment_tests) in;
#endif

void main() {
#ifdef CURSED_PREPASS
#ifdef ALPHA_CUTOUT
    // Sampled as the game samples it, so leaves keep their edges.
    float alpha = cursed_sample(Sampler0, texCoord0, 1.0f / TextureSize).a * vertexColor.a;
    if (alpha < ALPHA_CUTOUT) {
        discard;
    }
#endif
#else
    vec3 n = normalize(cross(dFdx(cursedPos), dFdy(cursedPos)));
    if (dot(n, cursedPos) > 0.0) {
        n = -n;
    }
    vec4 color = cursed_sample(Sampler0, texCoord0, 1.0f / TextureSize) * vertexColor;
#ifdef ALPHA_CUTOUT
    // Also in the lit pass: a quad over another at the same depth (the grass block's side overlay over its dirt)
    // passes the equal-depth test everywhere, its clear pixels included — those must not be drawn.
    if (color.a < ALPHA_CUTOUT) {
        discard;
    }
#endif
    vec3 albedo = color.rgb;
    if ((CursedSize.z & (1 << 28)) != 0) {
        // CURSED_DEV_FLATSHADE: the game's light only (what the engine's light costs, by difference).
        fragColor = vec4(color.rgb * cursed_lightmap(cursedLight), 1.0);
        return;
    }
    vec3 lit = cursed_light(cursedPos, cursedLight, n);
    color.rgb *= lit;
#ifndef CURSED_TRANSLUCENT
    // The block this pixel belongs to (class and material), read once for the metal's shine and the glow.
    uint own = 0u;
    if ((CursedSize.z & (4 | 128)) != 0 && length(cursedPos) < 96.0) {
        ivec3 ownCell = ivec3(floor(cursedPos - CameraOffset - n * 0.05)) + CameraBlockPos;
        own = cursed_inside(ownCell) ? cursed_voxel(ownCell) : 0u;
    }
    color.rgb = cursed_wet(color.rgb, cursedPos, cursedLight, n);
    color.rgb = cursed_metal(color.rgb, albedo, lit, cursedPos, n, own);
#endif
#ifdef CURSED_TRANSLUCENT
    if ((CursedSize.z & 4) != 0) {
        // Water and glass mirror the world: more the flatter they are seen (Fresnel).
        uint here = cursed_voxel(ivec3(floor(cursedPos - CameraOffset - n * 0.05)) + CameraBlockPos) & 15u;
        if (here == C_WATER || here == C_GLASS) {
            bool surface = here == C_WATER && n.y > 0.5;
            if (surface) {
                n = cursed_water_normal(cursedPos);
                // Crests lighter, troughs darker; the sun glints off the slopes.
                if ((CursedSize.z & 8) != 0 && max(abs(cursedPos.x), abs(cursedPos.z)) < 30.0) {
                    vec2 at = vec2(CameraBlockPos.xz & ivec2(63)) + cursedPos.xz - CameraOffset.xz;
                    float h = textureLod(CursedWater, at / 64.0, 0.0).x;
                    color.rgb *= 1.0 + clamp(h, -1.0, 1.0) * 0.22;
                    if ((CursedSize.z & 16) != 0) {
                        color = vec4(0.5 + h * 4.0, 0.5 - h * 4.0, 0.5, 1.0);
                    }
                }
                vec3 glintDir = reflect(normalize(cursedPos), n);
                color.rgb += CursedSunColor.rgb * pow(max(dot(glintDir, CursedSun.xyz), 0.0), 160.0) * 0.7 * CursedSun.w * (1.0 - CursedSky.a);
                // What lies under the water (the picture before it), bent by the waves and fading into the water's own
                // colour with depth: clear at the shore, deep blue far down, red going first.
                vec2 screenSize = vec2(textureSize(CursedScene, 0));
                vec2 straight = gl_FragCoord.xy / screenSize;
                vec2 bent = straight + n.xz * (0.9 / max(length(cursedPos), 1.0));
                float fragW = 1.0 / gl_FragCoord.w;
                float bentZ = textureLod(CursedSceneDepth, bent, 0.0).r;
                // Only what is behind the water may show through it.
                if (bentZ > gl_FragCoord.z || any(lessThan(bent, vec2(0.0))) || any(greaterThan(bent, vec2(1.0)))) {
                    bent = straight;
                    bentZ = textureLod(CursedSceneDepth, bent, 0.0).r;
                }
                float sceneW = bentZ > 0.0 ? ProjMat[3][2] / (bentZ + ProjMat[2][2]) : fragW + 64.0;
                float thickness = clamp(sceneW - fragW, 0.0, 64.0);
                vec3 below = textureLod(CursedScene, bent, 0.0).rgb;
                vec3 through = exp(-thickness * vec3(0.32, 0.12, 0.08));
                float seen = exp(-thickness * 0.18);
                vec3 waterColour = color.rgb;
                color.rgb = mix(waterColour, below * through * mix(vec3(1.0), normalize(waterColour + 1e-3) * 1.4, 0.35), seen);
                color.a = 1.0;
            }
            vec3 V = normalize(cursedPos);
            vec3 R = reflect(V, n);
            float f0 = here == C_WATER ? 0.02 : 0.04;
            float F = f0 + (1.0 - f0) * pow(1.0 - clamp(dot(-V, n), 0.0, 1.0), 5.0);
            // Seen from above a mirror shows little (a few percent): the sky's colour does. Otherwise what the picture
            // shows first, the voxels only where it shows nothing (off the screen, behind something).
            vec3 mirrored;
            if (F < 0.04) {
                mirrored = cursed_sky(R);
            } else {
                float onScreen;
                mirrored = cursed_screen_reflection(cursedPos + n * 0.05, R, onScreen);
                if (onScreen < 0.99) {
                    mirrored = mix(cursed_reflection(cursedPos - CameraOffset + n * 0.05, R), mirrored, onScreen);
                }
            }
            color.rgb = mix(color.rgb, mirrored, F);
            color.a = mix(color.a, 1.0, F);
            if (surface) {
                float foam = cursed_foam(cursedPos);
                color.rgb = mix(color.rgb, vec3(0.93, 0.96, 1.0) * lit, foam * 0.85);
                color.a = mix(color.a, 1.0, foam);
            }
        }
    }
#endif
    color = mix(FogColor * vec4(1, 1, 1, color.a), color, chunkVisibility);
    fragColor = apply_fog(color, sphericalVertexDistance, cylindricalVertexDistance, FogEnvironmentalStart, FogEnvironmentalEnd, FogRenderDistanceStart, FogRenderDistanceEnd, FogColor);
#ifndef CURSED_TRANSLUCENT
    // How much this pixel glows, for the glow round glowing blocks (glow.comp): in the picture's alpha, 1 none, below
    // 0.5 glowing (0.5 barely, 0 full). Opaque terrain is not blended, its alpha is free; what is blended over it later
    // (clouds over the sky, water) only raises it.
    float glowing = 0.0;
    if ((CursedSize.z & 128) != 0) {
        uint material = own >> 8;
        if (material >= 216u) {
            float bright = max(color.r, max(color.g, color.b));
            glowing = texelFetch(CursedPalette, ivec2(int(material), 1), 0).a * smoothstep(0.35, 0.85, bright)
                    * (1.0 - smoothstep(48.0, 96.0, length(cursedPos)));
        }
    }
    fragColor.a = glowing > 0.0 ? 0.5 * (1.0 - glowing) : 1.0;
#endif
#endif
}
