package cursed.recode.scene;

import com.mojang.blaze3d.systems.RenderSystem;
import cursed.recode.dev.Dev;
import net.minecraft.util.LightCoordsUtil;
import net.minecraft.core.BlockPos;
import net.minecraft.util.Mth;
import net.minecraft.world.level.Level;

/**
 * Particles' light looked up once per block per frame: each particle asks the light engine for its block's light
 * every frame (Particle.getLightCoords), and particles crowd into few blocks. While the particles are extracted
 * (ParticleEngine.extract) the level does not change, so the block's first answer is every particle's answer.
 * CURSED_DEV_NOPARTICLELIGHT asks for each.
 */
public final class ParticleLight {
    private static final boolean OFF = Dev.on("CURSED_DEV_NOPARTICLELIGHT");
    private static final int SIZE = 2048;
    private static final int PROBES = 8;
    private static final long[] keys = new long[SIZE];
    private static final int[] values = new int[SIZE];
    private static final int[] frames = new int[SIZE];
    private static int frame;
    private static boolean extracting;

    private ParticleLight() {
    }

    /** ParticleEngine.extract begins. */
    public static void begin() {
        frame++;
        extracting = !OFF && RenderSystem.isOnRenderThread();
    }

    public static void end() {
        extracting = false;
    }

    public static boolean on() {
        return extracting;
    }

    /** Particle.getLightCoords: the same expression, kept per block for the frame. */
    public static int light(Level level, double x, double y, double z) {
        int bx = Mth.floor(x);
        int by = Mth.floor(y);
        int bz = Mth.floor(z);
        long key = BlockPos.asLong(bx, by, bz);
        int slot = (int) (mix(key) & (SIZE - 1));
        int free = -1;
        for (int i = 0; i < PROBES; i++) {
            int s = (slot + i) & (SIZE - 1);
            if (frames[s] != frame) {
                if (free < 0) {
                    free = s;
                }
                break;
            }
            if (keys[s] == key) {
                return values[s];
            }
        }
        BlockPos pos = new BlockPos(bx, by, bz);
        int light = level.hasChunkAt(pos) ? LightCoordsUtil.getLightCoords(level, pos) : 15728640;
        if (free >= 0) {
            keys[free] = key;
            values[free] = light;
            frames[free] = frame;
        }
        return light;
    }

    private static long mix(long key) {
        long h = key * 0x9E3779B97F4A7C15L;
        return h ^ (h >>> 29);
    }
}
