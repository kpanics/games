package cursed.recode.render;

import com.mojang.renderpearl.backend.api.SpvModule;
import cursed.recode.dev.Dev;
import java.util.Collections;
import java.util.Set;
import java.util.WeakHashMap;
import org.jspecify.annotations.Nullable;

/**
 * Terrain vertices in 16 bytes instead of the game's 28 (vk/terrain.rs packs them on upload): position as three 16-bit
 * values in 1/2048 of a block from -8, the light as two bytes (the game's values are 0-240), UVs as two 16-bit values
 * in 1/32768, colour as it is. The game's
 * terrain vertex shader is patched where it is compiled (GlslCompilerMixin) to unpack them, and pipelines with the
 * patched shader read the packed layout (CursedPipeline). The picture moves by less than 1/4000 of a block — not the
 * game's bytes, as she decided (07.10). CURSED_DEV_NOCOMPACT keeps the game's format.
 */
public final class TerrainCompact {
    public static final String SHADER = "minecraft:core/terrain";
    public static final boolean OFF = Dev.on("CURSED_DEV_NOCOMPACT");
    /** The packed vertex's size (vk/terrain.rs PACKED_SIZE). */
    public static final int STRIDE = 16;
    private static final Set<SpvModule> PATCHED = Collections.synchronizedSet(Collections.newSetFromMap(new WeakHashMap<>()));

    private TerrainCompact() {
    }

    /** The terrain vertex shader unpacking the packed attributes, or null when its text is not what is expected. */
    public static @Nullable String patch(String source) {
        if (OFF || source.contains("cursed_Position") || !source.matches("(?s).*\\bin\\s+vec3\\s+Position\\s*;.*") || !source.matches("(?s).*\\bin\\s+vec2\\s+UV0\\s*;.*")
                || !source.matches("(?s).*\\bin\\s+ivec2\\s+UV2\\s*;.*")) {
            return null;
        }
        int main = source.indexOf("void main()");
        int brace = main < 0 ? -1 : source.indexOf('{', main);
        if (brace < 0) {
            return null;
        }
        // The light comes as two bytes read signed (R8G8_SINT: the game checks the input's name and its integer type),
        // so 128-240 arrive negative; the low eight bits are the game's value.
        String head = source.substring(0, brace + 1);
        String body = source.substring(brace + 1).replaceAll("\\bPosition\\b", "cursed_Position").replaceAll("\\bUV0\\b", "cursed_UV0")
                .replaceAll("\\bUV2\\b", "cursed_UV2");
        // One matrix by a vector after the other, not the two matrices multiplied for every vertex (measured 12.10:
        // 12 µs of a frame's terrain on an RTX 4060).
        body = body.replace("ProjMat * ModelViewMat * vec4(pos, 1.0)", "ProjMat * (ModelViewMat * vec4(pos, 1.0))");
        body = devExperiment(body);
        // Merged faces (vk/terrain.rs merge_plane): where in the plane, and how the sprite repeats (the colour's alpha;
        // the game's own vertices have 255). Their alpha is the game's again (the light map's is 1).
        int at = head.lastIndexOf("void main()");
        head = head.substring(0, at) + "layout(location = 5) out vec2 cursedTile;\nlayout(location = 6) flat out int cursedTiled;\n\n" + head.substring(at);
        int end = body.lastIndexOf('}');
        body = body.substring(0, end) + "    if (cursedTiled != 0) {\n        vertexColor.a = 1.0;\n    }\n" + body.substring(end);
        // UNORM16 reads value / 65535: back to 1/2048 of a block from -8, and to 1/32768 of the atlas.
        return head + "\n    vec3 cursed_Position = Position.xyz * (65535.0 / 2048.0) - 8.0;\n    vec2 cursed_UV0 = UV0 * (65535.0 / 32768.0);"
                + "\n    ivec2 cursed_UV2 = UV2 & 255;"
                + "\n    cursedTile = cursed_Position.zx;\n    int cursed_code = int(Color.a * 255.0 + 0.5);\n    cursedTiled = cursed_code < 128 ? 128 + cursed_code : 0;"
                + body;
    }

    /** The repeated sprite of merged faces in the fragment shader (vk/terrain.rs merge_plane). */
    private static final String TILE_GLSL = String.join("\n",
            "// A rectangle of merged faces (vk/terrain.rs merge_plane) repeats its sprite once a block: cursedTiled is 128 + how",
            "// (bit 0 u runs along the second in-plane axis, bits 1-2 u and v run backwards, bits 3-4 and 5-6 the sprite's size as",
            "// 2^-(5 + n) of the atlas), 0 for the game's own faces; cursedTile is where in the plane, in blocks; the UV is the",
            "// sprite's corner. The texture's derivatives come from the unrepeated coordinates, so mip levels match across blocks.",
            "layout(location = 5) in vec2 cursedTile;",
            "layout(location = 6) flat in int cursedTiled;",
            "",
            "vec4 cursed_sampleRGSS(sampler2D source, vec2 uv, vec2 pixelSize, vec2 du, vec2 dv) {",
            "    vec2 texelScreenSize = sqrt(du * du + dv * dv);",
            "    float maxTexelSize = max(texelScreenSize.x, texelScreenSize.y);",
            "    float minPixelSize = min(pixelSize.x, pixelSize.y);",
            "    float blendFactor = smoothstep(minPixelSize * 1.0, minPixelSize * 2.0, maxTexelSize);",
            "    float duLength = length(du);",
            "    float dvLength = length(dv);",
            "    float effectiveDerivative = sqrt(min(duLength, dvLength) * max(duLength, dvLength));",
            "    float mipLevelExact = max(0.0, log2(effectiveDerivative / minPixelSize));",
            "    float mipLevelLow = floor(mipLevelExact);",
            "    float mipLevelHigh = mipLevelLow + 1.0;",
            "    float mipBlend = fract(mipLevelExact);",
            "    const vec2 offsets[4] = vec2[](vec2(0.125, 0.375), vec2(-0.125, -0.375), vec2(0.375, -0.125), vec2(-0.375, 0.125));",
            "    vec4 rgssColorLow = vec4(0.0);",
            "    vec4 rgssColorHigh = vec4(0.0);",
            "    for (int i = 0; i < 4; ++i) {",
            "        vec2 sampleUV = uv + offsets[i] * pixelSize;",
            "        rgssColorLow += textureLod(source, sampleUV, mipLevelLow);",
            "        rgssColorHigh += textureLod(source, sampleUV, mipLevelHigh);",
            "    }",
            "    vec4 rgssColor = mix(rgssColorLow * 0.25, rgssColorHigh * 0.25, mipBlend);",
            "    vec4 nearestColor = sampleNearest(source, uv, pixelSize, du, dv, texelScreenSize);",
            "    return mix(nearestColor, rgssColor, blendFactor);",
            "}",
            "",
            "vec4 cursed_sample(sampler2D source, vec2 uv, vec2 pixelSize) {",
            "    if (cursedTiled == 0) {",
            "        return UseRgss == 1 ? sampleRGSS(source, uv, pixelSize) : sampleNearest(source, uv, pixelSize);",
            "    }",
            "    int c = cursedTiled - 128;",
            "    vec2 t = (c & 1) != 0 ? cursedTile.yx : cursedTile;",
            "    vec2 size = vec2(1.0 / float(1 << (5 + ((c >> 3) & 3))), 1.0 / float(1 << (5 + ((c >> 5) & 3))));",
            "    bvec2 back = bvec2((c & 2) != 0, (c & 4) != 0);",
            "    vec2 f = fract(t);",
            "    f = mix(f, 1.0 - f, back);",
            "    vec2 along = mix(t, -t, back);",
            "    vec2 du = dFdx(along * size);",
            "    vec2 dv = dFdy(along * size);",
            "    vec2 at = uv + f * size;",
            "    return UseRgss == 1 ? cursed_sampleRGSS(source, at, pixelSize, du, dv) : sampleNearest(source, at, pixelSize, du, dv, sqrt(du * du + dv * dv));",
            "}") + "\n";

    /** The terrain fragment shader sampling through cursed_sample (merged faces repeat their sprite), or as it is. */
    public static String patchFragment(String source) {
        String sample = "(UseRgss == 1 ? sampleRGSS(Sampler0, texCoord0, 1.0f / TextureSize) : sampleNearest(Sampler0, texCoord0, 1.0f / TextureSize))";
        int main = source.indexOf("void main()");
        if (OFF || source.contains("cursed_sample") || main < 0 || !source.contains(sample)) {
            return source;
        }
        return source.substring(0, main) + TILE_GLSL + "\n" + source.substring(main).replace(sample, "cursed_sample(Sampler0, texCoord0, 1.0f / TextureSize)");
    }

    /** Dev (CURSED_DEV_VSEXP=min|kill): the vertex shader made cheaper in a way to measure what its parts cost. */
    private static String devExperiment(String body) {
        String exp = System.getenv("CURSED_DEV_VSEXP");
        if (exp == null) {
            return body;
        }
        if (exp.contains("kill")) {
            // every triangle a point: what the geometry costs before the rasterizer
            int end = body.lastIndexOf('}');
            body = body.substring(0, end) + "    gl_Position = vec4(0.0, 0.0, 0.0, 1.0);\n" + body.substring(end);
        }
        if (exp.contains("min")) {
            body = body.replace("vertexColor = Color * sample_lightmap(Sampler2, UV2);", "vertexColor = Color;")
                    .replace("sphericalVertexDistance = fog_spherical_distance(pos);", "sphericalVertexDistance = 0.0;")
                    .replace("cylindricalVertexDistance = fog_cylindrical_distance(pos);", "cylindricalVertexDistance = 0.0;");
        }
        return body;
    }

    /** Dev (CURSED_DEV_FSEXP=min): the terrain fragment shader at its least (one sample, the cutout's test). */
    public static String devFragment(String source) {
        String exp = System.getenv("CURSED_DEV_FSEXP");
        if (exp == null || !exp.contains("min")) {
            return source;
        }
        return source.replace("(UseRgss == 1 ? sampleRGSS(Sampler0, texCoord0, 1.0f / TextureSize) : sampleNearest(Sampler0, texCoord0, 1.0f / TextureSize))", "texture(Sampler0, texCoord0)")
                .replace("fragColor = calculateFinalColor(color);", "fragColor = color;");
    }

    /**
     * The patched shader exists: terrain uploaded from now on is packed. Said here, where the resources load, because
     * the game makes the pipeline itself only when it first draws with it — after the first sections were uploaded.
     */
    public static void mark(SpvModule module) {
        PATCHED.add(module);
        CursedDevice d = CursedDevice.current;
        if (d != null) {
            Native.terrainCompact(d.r, true);
        }
    }

    /** A terrain shader that could not be patched: terrain goes in the game's format. */
    public static void unpatched() {
        CursedDevice d = CursedDevice.current;
        if (d != null) {
            Native.terrainCompact(d.r, false);
        }
    }

    private static final byte[] MARK = "cursed_Position".getBytes(java.nio.charset.StandardCharsets.US_ASCII);

    /**
     * Whether the module is the patched shader. The game hands pipelines its own module objects, so this looks at the
     * SPIR-V itself: the patch's variable name is in it (the game compiles with debug names).
     */
    static boolean isPatched(SpvModule module) {
        if (PATCHED.contains(module)) {
            return true;
        }
        java.nio.ByteBuffer spv = module.spv();
        int start = spv.position();
        int end = spv.limit() - MARK.length;
        outer:
        for (int i = start; i <= end; i++) {
            for (int k = 0; k < MARK.length; k++) {
                if (spv.get(i + k) != MARK[k]) {
                    continue outer;
                }
            }
            return true;
        }
        return false;
    }
}
