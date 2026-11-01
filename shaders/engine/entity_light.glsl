// Light for creatures and things (cursed.recode.render.EntityEngine patches the game's core/entity with this): the
// game's light at the vertex, with the sky's part shaded where the sun (or the moon) does not reach this point —
// traced through the voxel window, so a mob under a tree is in its shade — and the blocks' light coloured by what
// gives it. Needs CursedEngine (with the camera), the voxel window and the light grids declared, and trace.glsl.

vec4 cursed_entity_light(vec4 full, vec4 blockOnly, vec3 pos, vec3 normal) {
    vec3 block = blockOnly.rgb;
    vec3 sky = max(full.rgb - block, vec3(0.0));
    float nl = length(normal);
    vec3 n = nl > 1e-4 ? normal / nl : vec3(0.0, 1.0, 0.0);
    // The blocks' light in the colour of the glow nearby (shaders/engine/light.comp).
    float near = (1.0 - smoothstep(52.0, 62.0, max(abs(pos.x), abs(pos.z)))) * (1.0 - smoothstep(26.0, 31.0, abs(pos.y)));
    if ((CursedSize.z & 2) != 0 && near > 0.0) {
        vec3 at = vec3(CameraBlockPos & ivec3(127, 63, 127)) + pos - CameraOffset + n * 0.4;
        vec3 glow = textureLod(CursedLight, at / vec3(128.0, 64.0, 128.0), 0.0).rgb;
        float strongest = max(glow.r, max(glow.g, glow.b));
        if (strongest > 0.02) {
            vec3 hue = glow / strongest;
            float lum = dot(hue, vec3(0.2126, 0.7152, 0.0722));
            hue = mix(vec3(lum), hue, 0.75) / max(lum, 0.4);
            block = mix(block, block * hue, 0.8 * near * smoothstep(0.0, 0.35, strongest));
        }
    }
    if ((CursedSize.z & 1) == 0 || dot(sky, vec3(0.333)) < 0.002) {
        return vec4(block + sky, full.a);
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
    strength *= (1.0 - smoothstep(96.0, 124.0, max(abs(pos.x), abs(pos.z)))) * (1.0 - 0.85 * CursedSky.a);
    if (strength < 0.01) {
        return vec4(block + sky, full.a);
    }
    // Things are not in the voxels: they do not shade themselves, only the world shades them.
    float vis = cursed_trace(pos - CameraOffset + n * 0.05 + vec3(0.0, 0.05, 0.0), cursed_soft(L, uvec3(uvec2(gl_FragCoord.xy), uint(CursedSpare.x))), 96.0)
            * cursed_cloud_shadow(pos - CameraOffset, L);
    float shade = mix(1.0, mix(CursedSunColor.a, 1.0, vis), strength);
    vec3 color = mix(vec3(1.0), mix(vec3(0.86, 0.93, 1.1), tint, vis), strength);
    return vec4(block + sky * shade * color, full.a);
}
