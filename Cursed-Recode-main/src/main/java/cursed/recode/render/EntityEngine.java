package cursed.recode.render;

import com.mojang.logging.LogUtils;
import com.mojang.renderpearl.api.pipeline.ShaderType;
import com.mojang.renderpearl.backend.api.SpvModule;
import cursed.recode.dev.Dev;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.WeakHashMap;
import net.minecraft.client.renderer.ShaderDefines;
import org.jspecify.annotations.Nullable;
import org.slf4j.Logger;

/**
 * Creatures, players and things lit by the engine (native/shaders/engine/entity_light.glsl): the game's core/entity
 * shaders are compiled once more with the engine's light in (GlslCompilerMixin), next to the game's own; a pipeline
 * whose vertex and fragment shaders both have such a twin is made with the twins (CursedPipeline), their bindings set
 * to the game's places and the engine's after them. Pipelines without twins (glint, see-through phases, glowing ones)
 * keep the game's light. CURSED_DEV_NOENTITYLIGHT keeps the game's for all. A pipeline with CURSED_CHAMS defined
 * (cursed.recode.chams.Chams) has its matter in its twin as well (native/shaders/engine/chams.glsl), and is drawn with
 * its twin whatever the engine draws (vk/pipeline.rs).
 */
public final class EntityEngine {
    private static final Logger LOGGER = LogUtils.getLogger();
    public static final String SHADER = "minecraft:core/entity";
    /** Items in the hand, in frames and on the ground: the same light. */
    public static final String ITEM = "minecraft:core/item";
    /** Particles: the same light, facing the camera. */
    public static final String PARTICLE = "minecraft:core/particle";
    /** Clouds: lit by the sun's side (only their vertex shader has a twin). */
    public static final String CLOUDS = "minecraft:core/clouds";
    /** The sky: shaped by the air (native/shaders/engine/sky.glsl). */
    public static final String SKY = "minecraft:core/sky";
    private static final boolean NO_CLOUDS = Dev.on("CURSED_DEV_NOCLOUDLIGHT");
    private static final boolean OFF = EngineShaders.OFF || Dev.on("CURSED_DEV_NOENTITYLIGHT");
    /** In the twins' text: the compiler must not make twins of twins. */
    public static final String MARK = "CURSED_ENTITY_ENGINE";
    /** The define a chams' pipeline has (cursed.recode.chams.Chams): its kind. */
    public static final String CHAMS = "CURSED_CHAMS";
    private static final Map<SpvModule, SpvModule> TWINS = Collections.synchronizedMap(new WeakHashMap<>());

    private EntityEngine() {
    }

    /** Whether the game's shader gets a lit twin: core/entity, not glinting, not see-through phases, not glowing. */
    public static boolean wants(String name, ShaderType type, ShaderDefines defines, String source) {
        boolean known = SHADER.equals(name) || ITEM.equals(name) || PARTICLE.equals(name) || SKY.equals(name)
                || (CLOUDS.equals(name) && type == ShaderType.VERTEX && !NO_CLOUDS);
        if (OFF || !known || source.contains(MARK) || CursedDevice.current == null) {
            return false;
        }
        for (String f : defines.flags()) {
            if (f.startsWith("OIT") || f.equals("GLINT") || f.equals("EMISSIVE")) {
                return false;
            }
        }
        for (String k : defines.values().keySet()) {
            if (k.startsWith("OIT") || k.equals("GLINT") || k.equals("EMISSIVE")) {
                return false;
            }
        }
        return true;
    }

    public static void remember(SpvModule game, SpvModule twin) {
        TWINS.put(game, twin);
    }

    static @Nullable SpvModule twin(SpvModule game) {
        return TWINS.get(game);
    }

    static boolean isTwin(SpvModule module) {
        return TWINS.containsValue(module);
    }

    private static final String ENGINE_BLOCK = "layout(std140) uniform CursedEngine {\n    ivec4 CursedWindow;\n    ivec4 CursedSize;\n    vec4 CursedSun;\n    vec4 CursedSunColor;\n"
            + "    vec4 CursedMoon;\n    vec4 CursedSky;\n    vec4 CursedTime;\n    vec4 CursedSpare;\n    ivec4 CursedColumns[64];\n"
            + "    ivec4 CursedCameraBlock;\n    vec4 CursedCameraFrac;\n    vec4 CursedEntities[64];\n    vec4 CursedExtra;\n"
            + "    vec4 CursedChamsTint;\n    vec4 CursedChamsLook;\n    vec4 CursedChamsMore[4];\n};\n";

    /** What the engine's light needs in a fragment shader: its uniforms, the voxel window, the trace, the light. */
    private static String lightHead(CursedDevice d) {
        return ENGINE_BLOCK
                + "uniform usampler2DArray CursedVoxels;\nuniform usampler2DArray CursedBricks;\nuniform sampler3D CursedLight;\nuniform sampler2D CursedPalette;\n"
                + "#define CURSED_CLOUD_SHADOWS\n"
                + "#define CameraBlockPos (CursedCameraBlock.xyz)\n#define CameraOffset (-CursedCameraFrac.xyz)\n"
                + Native.text(d.r, 302) + "\n" + Native.text(d.r, 303) + "\n";
    }

    /** The game's text with the engine's light in (and a chams' matter), or null when it is not the text this expects. */
    public static @Nullable String patch(String name, String source, ShaderType type, ShaderDefines defines) {
        String ext = "#extension GL_ARB_separate_shader_objects : require\n";
        int at = source.indexOf(ext);
        if (at < 0) {
            return null;
        }
        at += ext.length();
        CursedDevice device = CursedDevice.current;
        if (device == null) {
            return null;
        }
        if (PARTICLE.equals(name)) {
            if (type == ShaderType.VERTEX) {
                String lit = "    vertexColor = Color * sample_lightmap(Sampler2, UV2);\n";
                if (!source.contains(lit)) {
                    return null;
                }
                String head = "// " + MARK + "\nlayout(location = 8) out vec3 cursedPos;\nlayout(location = 9) out vec4 cursedFull;\n"
                        + "layout(location = 10) out vec4 cursedBlockLight;\n";
                String body = "    vertexColor = Color;\n    cursedFull = sample_lightmap(Sampler2, UV2);\n"
                        + "    cursedBlockLight = sample_lightmap(Sampler2, ivec2(UV2.x, 0));\n    cursedPos = Position;\n";
                return source.substring(0, at) + head + source.substring(at).replace(lit, body);
            }
            String sample = "    vec4 color = texture(Sampler0, texCoord0) * vertexColor * ColorModulator;\n";
            if (!source.contains(sample)) {
                return null;
            }
            String head = "// " + MARK + "\nlayout(location = 8) in vec3 cursedPos;\nlayout(location = 9) in vec4 cursedFull;\n"
                    + "layout(location = 10) in vec4 cursedBlockLight;\n" + lightHead(device);
            String body = "    vec4 color = texture(Sampler0, texCoord0) * vertexColor * ColorModulator\n"
                    + "        * cursed_entity_light(cursedFull, cursedBlockLight, cursedPos, -normalize(cursedPos));\n";
            return source.substring(0, at) + head + source.substring(at).replace(sample, body);
        }
        if (SKY.equals(name)) {
            if (type == ShaderType.VERTEX) {
                String pos = "    gl_Position = ProjMat * ModelViewMat * vec4(Position, 1.0);\n";
                if (!source.contains(pos)) {
                    return null;
                }
                return source.substring(0, at) + "// " + MARK + "\nlayout(location = 8) out vec3 cursedDir;\n"
                        + source.substring(at).replace(pos, pos + "    cursedDir = Position;\n");
            }
            String fog = "    fragColor = apply_fog(ColorModulator,";
            if (!source.contains(fog)) {
                return null;
            }
            // The sky's shape is read from the engine's table (native/shaders/engine/sky.glsl, in the palette).
            String head = "// " + MARK + "\nlayout(location = 8) in vec3 cursedDir;\n" + ENGINE_BLOCK
                    + "uniform sampler2D CursedPalette;\n#define CURSED_SKY_LUT\n" + Native.text(device.r, 304) + "\n";
            return source.substring(0, at) + head + source.substring(at)
                    .replace(fog, "    fragColor = apply_fog(vec4(cursed_sky_shape(ColorModulator.rgb, cursedDir), ColorModulator.a),");
        }
        if (CLOUDS.equals(name)) {
            String face = "    vertexColor = (useTopColor ? faceColors[1] : faceColors[direction]) * CloudColor;\n";
            if (!source.contains(face)) {
                return null;
            }
            String head = "// " + MARK + "\n" + ENGINE_BLOCK + """
                    // Clouds lit from the sun's side: warm where it shines (pink at sunset), cooler away from it.
                    vec3 cursed_cloud_light(int direction, bool top) {
                        if ((CursedSize.z & 256) == 0) {
                            return vec3(1.0);
                        }
                        const vec3 NORMALS[6] = vec3[](vec3(0, -1, 0), vec3(0, 1, 0), vec3(0, 0, -1), vec3(0, 0, 1), vec3(-1, 0, 0), vec3(1, 0, 0));
                        vec3 n = top ? vec3(0.0, 1.0, 0.0) : NORMALS[clamp(direction, 0, 5)];
                        bool day = CursedSun.w > 0.02;
                        vec3 L = day ? CursedSun.xyz : CursedMoon.xyz;
                        float strength = (day ? CursedSun.w : CursedMoon.w) * (1.0 - 0.6 * CursedSky.a);
                        float facing = smoothstep(-0.25, 0.65, dot(n, L));
                        vec3 sun = day ? CursedSunColor.rgb * 1.06 : vec3(0.85, 0.9, 1.05);
                        vec3 lit = mix(vec3(0.8, 0.85, 0.96), sun, facing);
                        return mix(vec3(1.0), lit, strength * 0.85);
                    }
                    """;
            String body = face + "    vertexColor.rgb *= cursed_cloud_light(direction, useTopColor);\n";
            return source.substring(0, at) + head + source.substring(at).replace(face, body);
        }
        String chams = defines.values().get(CHAMS);
        if (type == ShaderType.VERTEX) {
            String pos = "    gl_Position = ProjMat * ModelViewMat * vec4(Position, 1.0);\n";
            if (!source.contains(pos) || !source.contains(" in vec3 Normal;")) {
                return null;
            }
            String head = "// " + MARK + "\nlayout(location = 8) out vec3 cursedPos;\nlayout(location = 9) out vec3 cursedNormal;\n"
                    + "layout(location = 10) out vec4 cursedBlockLight;\n";
            String body = pos + "    cursedPos = Position;\n    cursedNormal = Normal;\n"
                    + "#if !defined(EMISSIVE) && !defined(OIT_ALPHA_ONLY)\n    cursedBlockLight = sample_lightmap(Sampler2, ivec2(UV2.x, 0));\n"
                    + "#else\n    cursedBlockLight = vec4(1.0);\n#endif\n";
            String text = source.substring(at);
            String before = source.substring(0, at);
            if (chams != null) {
                // the overlay as given (a copy's age rides in it: its fetch kept inside the overlay's 16 by 16), the
                // world's axes to the view's, the projection's scale
                if (!source.contains(" in ivec2 UV1;") || !before.startsWith("#version 330")) {
                    return null;
                }
                // the draw's first vertex (gl_BaseVertexARB) is there from GLSL 4.50
                before = "#version 450" + before.substring("#version 330".length());
                head = "#extension GL_ARB_shader_draw_parameters : require\n" + head
                        + "layout(location = 11) flat out ivec2 chamsOverlay;\nlayout(location = 12) flat out mat3 chamsRot;\n"
                        + "layout(location = 15) flat out vec4 chamsProj;\nlayout(location = 16) out vec2 chamsFace;\n";
                // where on its face a point is (0..1 each way): a face is four vertices in a row, any of them first —
                // counted from the draw's first vertex (where the draw lands in the frame's buffer changes every frame:
                // counted from the buffer's start, a face's corners went round, its pattern turning with them)
                body += "    chamsOverlay = UV1;\n    chamsRot = mat3(ModelViewMat);\n    chamsProj = vec4(ProjMat[0][0], ProjMat[1][1], ProjMat[2][2], ProjMat[3][2]);\n"
                        + "    chamsFace = vec2[](vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0))[(gl_VertexIndex - gl_BaseVertexARB) & 3];\n";
                text = text.replace("texelFetch(Sampler1, UV1, 0)", "texelFetch(Sampler1, clamp(UV1, ivec2(0), ivec2(15)), 0)");
            }
            return before + head + text.replace(pos, body);
        }
        String light = "    color *= lightMapColor;\n";
        if (!source.contains(light)) {
            return null;
        }
        String head = "// " + MARK + "\n"
                + "layout(location = 8) in vec3 cursedPos;\nlayout(location = 9) in vec3 cursedNormal;\nlayout(location = 10) in vec4 cursedBlockLight;\n"
                + lightHead(device);
        String lit = "    color *= cursed_entity_light(lightMapColor, cursedBlockLight, cursedPos, cursedNormal);\n";
        String text = source.substring(at).replace(light, lit);
        if (chams != null) {
            String main = "void main() {";
            String last = "    fragColor = calculateFinalColor(color);\n";
            String sample = "    vec4 color = texture(Sampler0, texCoord0);\n";
            String cut = "    if (color.a < ALPHA_CUTOUT) {\n        discard;\n    }\n";
            if (!text.contains(main) || !text.contains(last) || !text.contains(sample)) {
                return null;
            }
            // a clear pixel is left out only after the matter is worked out: left out at once, its neighbours'
            // derivatives (the rolled edges, the ripples) would break along every triangle's edge
            text = text.replace(cut, "    if (color.a < ALPHA_CUTOUT) {\n        chamsCut = true;\n    }\n");
            head += "layout(location = 11) flat in ivec2 chamsOverlay;\nlayout(location = 12) flat in mat3 chamsRot;\nlayout(location = 15) flat in vec4 chamsProj;\n"
                    + "layout(location = 16) in vec2 chamsFace;\nbool chamsCut = false;\n"
                    + "uniform sampler2D CursedScene;\nuniform sampler2D CursedSceneDepth;\n";
            // the matter after everything the shader declares, then the game's final colour through it, and the fog
            String finish = Native.text(device.r, 305) + """

                    vec4 cursed_chams_final(vec4 color) {
                        vec4 lit = calculateFinalColor(color);
                    #ifndef EMISSIVE
                        vec4 light = lightMapColor;
                    #else
                        vec4 light = vec4(1.0);
                    #endif
                        vec4 c = cursed_chams(color, lit, light);
                        if (chamsCut) {
                            discard;
                        }
                        return apply_fog(c, sphericalVertexDistance, cylindricalVertexDistance, FogEnvironmentalStart, FogEnvironmentalEnd,
                                FogRenderDistanceStart, FogRenderDistanceEnd, FogColor);
                    }

                    """;
            text = text.replace(main, finish + main).replace(last, "    fragColor = cursed_chams_final(color);\n");
        }
        return source.substring(0, at) + head + text;
    }

    /**
     * The twins' bindings: the game's uniforms in their places in the pipeline's list, the engine's after them (as
     * EngineShaders.EXTRA lists them). False when a twin uses a uniform that is in neither.
     */
    static boolean bind(SpvModule twin, List<String> game) {
        try {
            for (SpvModule.Reflection.Descriptor d : twin.reflect().descriptors()) {
                int index = game.indexOf(d.name());
                if (index < 0) {
                    int extra = -1;
                    for (int i = 0; i < EngineShaders.EXTRA.size(); i++) {
                        if (EngineShaders.EXTRA.get(i).getKey().equals(d.name())) {
                            extra = i;
                        }
                    }
                    if (extra < 0) {
                        LOGGER.warn("[engine] entity light: {} is not the pipeline's", d.name());
                        return false;
                    }
                    index = game.size() + extra;
                }
                d.binding(index);
                d.descriptorSetIndex(0);
            }
            return true;
        } catch (Exception e) {
            LOGGER.warn("[engine] entity light: the twin's bindings", e);
            return false;
        }
    }
}
