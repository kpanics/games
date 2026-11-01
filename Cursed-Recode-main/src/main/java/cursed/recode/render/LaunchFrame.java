package cursed.recode.render;

import com.mojang.renderpearl.api.textures.GpuTexture;

/** Hands the launch's frame (the main menu's picture, launch/mod.rs) to the DLL in its turn in the frame's commands. */
public final class LaunchFrame {
    public static final int LAUNCHED = 1;
    public static final int SMOKE_RESET = 2;
    public static final int NO_RAYS = 4;
    public static final int NO_BLOOM = 8;
    /** The fire under the words is the menus' (native/src/menus): the launch draws only its sparks. */
    public static final int MENUS_FIRE = 16;

    private LaunchFrame() {
    }

    /** v12's scenes (timeline.rs Phase): the launch and the menu, the way out of the menu, into a world, a world, out of one. */
    public static final int RUN = 0, EXIT_MENU = 1, ENTER = 2, WORLD = 3, EXIT_WORLD = 4;
    static int phase = RUN;
    static float phaseT, load = 1.0F, loadedAt = -1.0F;
    static long world;

    /**
     * The next frame's scene: which, its own second, how far the game or the world has loaded (0..1), when the world
     * had (Enter's seconds, negative: not yet), the world's frame (0 none). Taken by the next send, then back to the
     * launch's own.
     */
    public static void scene(int which, float seconds, float loaded, float loadedAtSeconds, @org.jspecify.annotations.Nullable GpuTexture worldFrame) {
        phase = which;
        phaseT = seconds;
        load = loaded;
        loadedAt = loadedAtSeconds;
        world = worldFrame != null ? CursedTexture.handle(worldFrame) : 0L;
    }

    /** Whether our renderer draws (the launch's picture needs it). */
    public static boolean active() {
        return CursedDevice.current != null;
    }

    public static void send(GpuTexture out, float t, float fx, float life, float dt, int flags, float gather, float[] cursor, float[] look, float boost, float flashC, float fade,
                            float ui, float left, float footTop, float[] hover, float hoverSparks, float[] lines, int lineCount, float[] icons, float[] glyphs, int glyphCount) {
        send(out, t, fx, life, dt, flags, gather, cursor, look, boost, flashC, fade, ui, left, footTop, hover, hoverSparks, lines, lineCount, icons, glyphs, glyphCount, null);
    }

    /** The same with the menus' screens' world: 24 knobs (launch.glsl Knob0..5) and the camera's place (5). */
    public static void send(GpuTexture out, float t, float fx, float life, float dt, int flags, float gather, float[] cursor, float[] look, float boost, float flashC, float fade,
                            float ui, float left, float footTop, float[] hover, float hoverSparks, float[] lines, int lineCount, float[] icons, float[] glyphs, int glyphCount,
                            float @org.jspecify.annotations.Nullable [] knobs) {
        CursedDevice d = CursedDevice.current;
        if (d != null) {
            d.encoder.launch(CursedTexture.handle(out), t, fx, life, dt, flags, gather, cursor, look, boost, flashC, fade, ui, left, footTop, hover, hoverSparks, lines, lineCount, icons, glyphs, glyphCount,
                    knobs, phase, phase == RUN ? t : phaseT, load, loadedAt, world);
        }
        phase = RUN;
        load = 1.0F;
        loadedAt = -1.0F;
        world = 0L;
    }
}
