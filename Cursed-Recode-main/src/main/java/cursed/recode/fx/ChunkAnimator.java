package cursed.recode.fx;

import java.util.Map;

/**
 * Chunk Animator (the menu's module "chunk-animator"): the world comes into sight moving in, a section at a time, as it
 * loads — rising from below, falling into place from above, or growing out of its middle, easing as it settles. Each
 * section's way in is the game's own fade-in (its first upload: a block placed later moves nothing), as long as set
 * here; the terrain's vertex shader moves it (shaders/engine/terrain.vsh, the engine's flags 2048/4096/8192).
 */
public final class ChunkAnimator {
    private static final int RISE = 2048;
    private static final int FALL = 4096;
    private static final int GROW = 8192;
    private static Map<String, String> set = Map.of();
    private static long polled;

    private ChunkAnimator() {
    }

    private static Map<String, String> settings() {
        long now = System.nanoTime();
        if (now - polled > 250_000_000L) {
            polled = now;
            set = Fx.settings("chunk-animator");
        }
        return set;
    }

    /** The engine's flags for the way sections come in (none when off). */
    public static int flags() {
        Map<String, String> v = settings();
        if (!Fx.on(v, "on", false)) {
            return 0;
        }
        return switch (Fx.choice(v, "Style", "Rise")) {
            case "Fall" -> FALL;
            case "Grow" -> GROW;
            default -> RISE;
        };
    }

    /** How long a section takes to come in (ms): this module's when on, the game's fade otherwise. */
    public static long length(long game) {
        Map<String, String> v = settings();
        return Fx.on(v, "on", false) ? Math.max(50L, (long) (Fx.number(v, "Length", 0.8F) * 1000.0F)) : game;
    }
}
