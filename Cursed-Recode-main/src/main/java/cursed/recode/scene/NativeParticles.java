package cursed.recode.scene;

import com.mojang.blaze3d.vertex.BufferBuilder;
import com.mojang.blaze3d.vertex.ByteBufferBuilder;
import com.mojang.blaze3d.vertex.DefaultVertexFormat;
import com.mojang.blaze3d.vertex.MeshData;
import com.mojang.blaze3d.vertex.VertexConsumer;
import com.mojang.logging.LogUtils;
import com.mojang.renderpearl.api.pipeline.PrimitiveTopology;
import cursed.recode.dev.Dev;
import cursed.recode.render.Native;
import cursed.recode.render.NativeTerrain;
import cursed.recode.world.VertexAppend;
import java.nio.ByteBuffer;
import java.util.concurrent.atomic.LongAdder;
import net.minecraft.client.particle.SingleQuadParticle;
import net.minecraft.client.renderer.state.level.QuadParticleRenderState;
import org.lwjgl.system.MemoryUtil;
import org.slf4j.Logger;

/**
 * Particles' quads made in the DLL (native/src/scene/particles.rs): QuadParticleRenderState.buildLayer into a
 * BufferBuilder of the particle format hands the layer's arrays over as they are — one call per layer instead of four
 * vertices of element-by-element writes per particle. Same bytes as the game (CURSED_DEV_MODELCHECK compares).
 */
public final class NativeParticles {
    private static final Logger LOGGER = LogUtils.getLogger();
    private static final boolean CHECK = Dev.on("CURSED_DEV_MODELCHECK");
    private static final boolean OFF = Dev.on("CURSED_DEV_NOMODELS");
    private static final LongAdder checked = new LongAdder();
    private static final LongAdder mismatched = new LongAdder();
    private static ByteBufferBuilder scratch;
    private static boolean game;

    private NativeParticles() {
    }

    /** buildLayer: true when the layer went natively. */
    public static boolean build(QuadParticleRenderState state, SingleQuadParticle.Layer layer, VertexConsumer consumer) {
        if (OFF || game || !(consumer instanceof BufferBuilder builder) || ((VertexAppend) builder).cursed$format() != DefaultVertexFormat.PARTICLE
                || !NativeTerrain.active()) {
            return false;
        }
        QuadParticleRenderState.Storage storage = (QuadParticleRenderState.Storage) state.particles.get(layer);
        if (storage == null) {
            return true;
        }
        int count = storage.currentParticleIndex;
        if (count == 0) {
            return true;
        }
        long dst = ((VertexAppend) builder).cursed$reserve(count * 4);
        int written = Native.particleQuads(storage.floatValues, storage.intValues, count, dst);
        if (written != count * 4) {
            throw new IllegalStateException("Particle vertices: wrote " + written + " of " + count * 4);
        }
        if (CHECK) {
            check(state, layer, dst, written);
        }
        return true;
    }

    /** Dev: the game's buildLayer into a scratch builder; the bytes must match. Render thread. */
    private static void check(QuadParticleRenderState state, SingleQuadParticle.Layer layer, long ours, int vertices) {
        if (scratch == null) {
            scratch = new ByteBufferBuilder(1 << 20);
        }
        BufferBuilder builder = new BufferBuilder(scratch, PrimitiveTopology.QUADS, DefaultVertexFormat.PARTICLE);
        game = true;
        try {
            state.buildLayer(layer, builder);
        } finally {
            game = false;
        }
        int differ = 0;
        String first = null;
        MeshData mesh = builder.build();
        try {
            ByteBuffer g = mesh.vertexBuffer();
            for (int i = 0; i < vertices * 28; i++) {
                byte x = MemoryUtil.memGetByte(ours + i);
                byte y = g.get(g.position() + i);
                if (x != y) {
                    if (first == null) {
                        first = "vertex " + i / 28 + " byte " + i % 28 + ": " + (x & 0xFF) + " vs the game's " + (y & 0xFF);
                    }
                    differ++;
                }
            }
        } finally {
            mesh.close();
        }
        checked.increment();
        if (differ > 0) {
            long n = mismatched.sum();
            mismatched.increment();
            if (n < 20) {
                LOGGER.warn("[particles] a layer differs from the game: {} bytes; first {}", differ, first);
            }
        }
        if (checked.sum() % 2000 == 0) {
            LOGGER.info("[particles] checked {} layers, {} differ", checked.sum(), mismatched.sum());
        }
    }
}
