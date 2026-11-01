package cursed.recode.render;

import cursed.recode.dev.Dev;

/**
 * The game's Improved Transparency without its most costly part. Mojang's order-independent transparency spreads each
 * pixel's translucent layers over eight depth bins between the nearest and the farthest of them, which takes a whole
 * pass over every translucent fragment just to find those two depths (Depth Bounds), and two full-screen passes over a
 * 32-bit float target to cull them (12.10 at 1440p, 15 chunks: some 470 µs of the 690 the feature costs). Here the bins
 * lie over a fixed range instead (0.05 to 16384 blocks, logarithmic: the farthest terrain stays out of the last bin,
 * whose step the wavelets give back only in part): the depth bounds are neither drawn nor culled
 * (vk/pass.rs leaves their draws out), the shaders find a fragment's bin from its own depth, and the composite leaves
 * the depth alone (it wrote the nearest bound). A pixel with one translucent layer (nearly all) comes out the same; with
 * layers close together the order between them is softer. CURSED_DEV_VANILLA_OIT: the game's own.
 */
public final class CheapOit {
    public static final boolean ON = !Dev.on("CURSED_DEV_VANILLA_OIT");
    /** The far water drawn the classic way (vk/terrain.rs far_water); CURSED_DEV_FARWATER=0: not. */
    public static final boolean FAR_WATER = !"0".equals(System.getenv("CURSED_DEV_FARWATER"));
    private static final String VANILLA = "float normalizeDepth(float fragmentDeviceDepth) {";

    private CheapOit() {
    }

    /** OIT shader text with the fixed-range depth in place of the per-pixel one, or as it is. */
    public static String patch(String source) {
        if (!ON) {
            return source;
        }
        // the composite: no depth to write (CursedPipeline keeps its depth write off), so no bound to read
        String depth = "float closestBoundDeviceDepth = texelFetch(DepthBoundsSampler, pixelCoords, 0).b;";
        if (source.contains(depth) && source.contains("gl_FragDepth = closestBoundDeviceDepth;")) {
            // and a pixel no translucent layer reached (most of them) goes before the coefficients are read: nothing
            // accumulated means no coverage and no light, which the game discards too, after reading them
            String fetch = "vec4 accumulatedColor = texelFetch(Sampler0, pixelCoords, 0);";
            return source.replace(depth, "").replace("gl_FragDepth = closestBoundDeviceDepth;", "")
                    .replace(fetch, fetch + "\n    if (accumulatedColor == vec4(0.0)) {\n        discard;\n    }");
        }
        if (!source.contains(VANILLA) || source.contains("cursed_normalizeDepthVanilla")) {
            return source;
        }
        String ours = String.join("\n",
                "// Cursed (cursed.recode.render.CheapOit): the bins over a fixed range, not each pixel's own.",
                "float normalizeDepth(float fragmentDeviceDepth) {",
                "    float d = max(deviceToLinearDepth(fragmentDeviceDepth), 0.05);",
                "    return clamp(log2(d / 0.05) / log2(16384.0 / 0.05), 0.0, 1.0);",
                "}",
                "") + "\n";
        return source.replace(VANILLA, ours + "float cursed_normalizeDepthVanilla(float fragmentDeviceDepth) {");
    }

    /** The composite writes no depth (it wrote the nearest depth bound, which is not made any more). */
    public static boolean keepsDepth(String pipeline) {
        return ON && pipeline.endsWith("pipeline/oit_composite");
    }
}
