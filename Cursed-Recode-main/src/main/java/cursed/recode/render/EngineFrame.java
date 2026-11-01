package cursed.recode.render;

/** Hands the engine's frame to the DLL in its turn in the frame's commands (CursedEncoder.engine). */
public final class EngineFrame {
    private EngineFrame() {
    }

    /** Chams' ink over the level's picture (native/src/engine/ink.rs): the fog's colour, the bodies (27 floats each). */
    public static void ink(com.mojang.renderpearl.api.textures.GpuTexture color, com.mojang.renderpearl.api.textures.GpuTexture depth, float[] fog, float[] bodies,
                           int count, float[] boxes, int boxCount) {
        CursedDevice d = CursedDevice.current;
        if (d != null) {
            d.encoder.ink(CursedTexture.handle(color), CursedTexture.handle(depth), fog, bodies, count, boxes, boxCount);
        }
    }

    /** Target ESP over the level's picture (native/src/engine/tesseract.rs): the targets (3 ints, 6 floats and their
     *  boxes each). */
    public static void target(com.mojang.renderpearl.api.textures.GpuTexture color, com.mojang.renderpearl.api.textures.GpuTexture depth, int[] ints, float[] floats,
                              float[][] boxes, int count) {
        CursedDevice d = CursedDevice.current;
        if (d != null) {
            d.encoder.target(CursedTexture.handle(color), CursedTexture.handle(depth), ints, floats, boxes, count);
        }
    }

    public static void hitboxes(com.mojang.renderpearl.api.textures.GpuTexture color, com.mojang.renderpearl.api.textures.GpuTexture depth, int[] ints, float[] floats, int count) {
        CursedDevice d = CursedDevice.current;
        if (d != null) {
            d.encoder.hitboxes(CursedTexture.handle(color), CursedTexture.handle(depth), ints, floats, count);
        }
    }

    /** The world's effects over the level's picture (native/src/engine/fx.rs): the shapes (20 floats each). */
    public static void fx(com.mojang.renderpearl.api.textures.GpuTexture color, com.mojang.renderpearl.api.textures.GpuTexture depth, float[] floats, int count) {
        CursedDevice d = CursedDevice.current;
        if (d != null) {
            d.encoder.fx(CursedTexture.handle(color), CursedTexture.handle(depth), floats, count);
        }
    }

    /** The cosmetics in the hands over the picture (native/src/cosmetics/view.rs): the frame's facts. */
    public static void viewmodel(com.mojang.renderpearl.api.textures.GpuTexture color, float yaw, float pitch, float vx, float vy, float vz, int flags, float sky, float block) {
        CursedDevice d = CursedDevice.current;
        if (d != null) {
            d.encoder.viewmodel(CursedTexture.handle(color), yaw, pitch, vx, vy, vz, flags, sky, block);
        }
    }

    /** CS's item in the player's hand seen from behind (native/src/cosmetics/view.rs third): the level's picture and
     *  depth, the hold where the game would draw its item (from the camera), the light at the player. */
    public static void third(com.mojang.renderpearl.api.textures.GpuTexture color, com.mojang.renderpearl.api.textures.GpuTexture depth, float[] pose, float sky, float block) {
        CursedDevice d = CursedDevice.current;
        if (d != null) {
            d.encoder.third(CursedTexture.handle(color), CursedTexture.handle(depth), pose, sky, block);
        }
    }

    /** Whether the engine draws this session (our renderer, and not switched off). */
    public static boolean active() {
        return CursedDevice.current != null && !EngineShaders.OFF;
    }

    public static void upscale(com.mojang.renderpearl.api.textures.GpuTexture color, com.mojang.renderpearl.api.textures.GpuTexture depth,
                               com.mojang.renderpearl.api.textures.GpuTexture out, float jx, float jy) {
        CursedDevice d = CursedDevice.current;
        if (d != null) {
            d.encoder.upscale(CursedTexture.handle(color), CursedTexture.handle(depth), CursedTexture.handle(out), jx, jy);
        }
    }

    public static void send(double cx, double cy, double cz, int minY, int height, float[] sun, float[] sunColor, float[] moon, float[] sky, float time, int flags, float scale, float[] mover,
            float[] casters, int casterCount, float cloudDrift, float cloudHeight, boolean clouds, float wet) {
        CursedDevice d = CursedDevice.current;
        if (d != null) {
            d.encoder.engine(cx, cy, cz, minY, height, sun, sunColor, moon, sky, time, flags, scale, mover, casters, casterCount, cloudDrift, cloudHeight, clouds, wet);
        }
    }
}
