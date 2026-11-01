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
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.atomic.LongAdder;
import net.minecraft.client.gui.font.TextRenderable;
import net.minecraft.client.gui.font.glyphs.BakedSheetGlyph;
import org.joml.Matrix4f;
import org.joml.Matrix4fc;
import org.lwjgl.system.MemoryUtil;
import org.slf4j.Logger;

/**
 * Text in the world (TextFeatureRenderer: name tags, holograms) and the GUI's (GuiGlyphRun) written by the DLL
 * (native/src/scene/text.rs): glyphs
 * that go one after another to the same builder with the same pose and light are one native call. Anything else — a
 * background, an underline, another consumer — first flushes the run, then goes the game's way, so every builder gets
 * its vertices in the game's order. Same bytes as the game (CURSED_DEV_MODELCHECK compares).
 */
public final class NativeText {
    private static final Logger LOGGER = LogUtils.getLogger();
    private static final boolean CHECK = Dev.on("CURSED_DEV_MODELCHECK");
    private static final boolean OFF = Dev.on("CURSED_DEV_NOMODELS");
    private static final int HEADER = 80;
    /** One glyph's record for the DLL (bytes). */
    public static final int GLYPH = 60;
    private static final LongAdder checked = new LongAdder();
    private static final LongAdder mismatched = new LongAdder();

    // Render thread only (TextFeatureRenderer runs there).
    private static final long header = MemoryUtil.nmemAlloc(HEADER);
    private static long glyphs = MemoryUtil.nmemAlloc(GLYPH * 256L);
    private static int capacity = 256;
    private static int count;
    private static int vertices;
    private static BufferBuilder builder;
    private static final Matrix4f pose = new Matrix4f();
    private static int light;
    private static boolean flat;
    private static com.mojang.renderpearl.api.vertex.VertexFormat format;
    private static final List<BakedSheetGlyph.GlyphInstance> pending = new ArrayList<>();
    private static ByteBufferBuilder scratch;
    /** Bumped by TextFeatureRenderer each time its pose is set (TextFeatureRendererMixin); the run's own, -1 when the
     * run's pose is compared by value (the GUI's). Comparing sixteen floats per glyph cost more than the glyph. */
    private static int poseEpoch;
    private static int runEpoch = -1;
    /** The last builder whose vertex format was looked at, and whether it takes glyphs. */
    private static BufferBuilder formatChecked;
    private static boolean formatTakes;

    private NativeText() {
    }

    /** TextFeatureRenderer set a new pose for the next submit. */
    public static void newPose() {
        poseEpoch++;
    }

    /** renderable.render(pose, consumer, light, flat) of TextFeatureRenderer: its pose changes only with newPose(). */
    public static void renderFeature(TextRenderable renderable, Matrix4fc pose, VertexConsumer consumer, int light, boolean flat) {
        if (!OFF && renderable instanceof BakedSheetGlyph.GlyphInstance glyph && consumer instanceof BufferBuilder b && textFormat(b) && NativeTerrain.active()) {
            if (count > 0 && (b != builder || light != NativeText.light || flat != NativeText.flat || runEpoch != poseEpoch)) {
                flush();
            }
            if (count == 0) {
                begin(b, pose, light, flat);
                runEpoch = poseEpoch;
            }
            add(glyph);
            return;
        }
        flush();
        renderable.render(pose, consumer, light, flat);
    }

    /** renderable.render(pose, consumer, light, flat) of GUI glyphs. */
    public static void render(TextRenderable renderable, Matrix4fc pose, VertexConsumer consumer, int light, boolean flat) {
        if (!OFF && renderable instanceof BakedSheetGlyph.GlyphInstance glyph && consumer instanceof BufferBuilder b && textFormat(b) && NativeTerrain.active()) {
            if (count > 0 && (b != builder || light != NativeText.light || flat != NativeText.flat || !NativeText.pose.equals(pose))) {
                flush();
            }
            if (count == 0) {
                begin(b, pose, light, flat);
                runEpoch = -1;
            }
            add(glyph);
            return;
        }
        flush();
        renderable.render(pose, consumer, light, flat);
    }

    private static boolean textFormat(BufferBuilder b) {
        if (b != formatChecked) {
            com.mojang.renderpearl.api.vertex.VertexFormat f = ((VertexAppend) b).cursed$format();
            formatChecked = b;
            formatTakes = f == DefaultVertexFormat.POSITION_TEX_LIGHTMAP_COLOR || f == DefaultVertexFormat.POSITION_TEX_COLOR;
        }
        return formatTakes;
    }

    private static void begin(BufferBuilder b, Matrix4fc m, int light, boolean flat) {
        builder = b;
        format = ((VertexAppend) b).cursed$format();
        pose.set(m);
        NativeText.light = light;
        NativeText.flat = flat;
        MemoryUtil.memPutInt(header, m.properties());
        MemoryUtil.memPutInt(header + 4, light);
        MemoryUtil.memPutInt(header + 8, flat ? 1 : 0);
        MemoryUtil.memPutInt(header + 12, format == DefaultVertexFormat.POSITION_TEX_COLOR ? 1 : 0);
        float[] v = {m.m00(), m.m01(), m.m02(), m.m03(), m.m10(), m.m11(), m.m12(), m.m13(), m.m20(), m.m21(), m.m22(), m.m23(), m.m30(), m.m31(), m.m32(), m.m33()};
        for (int i = 0; i < 16; i++) {
            MemoryUtil.memPutFloat(header + 16 + i * 4L, v[i]);
        }
    }

    private static void add(BakedSheetGlyph.GlyphInstance g) {
        if (count == capacity) {
            capacity *= 2;
            glyphs = MemoryUtil.nmemRealloc(glyphs, (long) GLYPH * capacity);
        }
        vertices += record(glyphs + (long) GLYPH * count, g);
        count++;
        if (CHECK) {
            pending.add(g);
        }
    }

    /** A glyph's record at r; the vertices it makes. */
    public static int record(long r, BakedSheetGlyph.GlyphInstance g) {
        BakedSheetGlyph s = g.glyph();
        boolean bold = g.style().isBold();
        boolean shadow = g.shadowColor() != 0;
        MemoryUtil.memPutFloat(r, g.x());
        MemoryUtil.memPutFloat(r + 4, g.y());
        MemoryUtil.memPutFloat(r + 8, g.shadowOffset());
        MemoryUtil.memPutFloat(r + 12, g.boldOffset());
        MemoryUtil.memPutFloat(r + 16, s.left);
        MemoryUtil.memPutFloat(r + 20, s.right);
        MemoryUtil.memPutFloat(r + 24, s.up);
        MemoryUtil.memPutFloat(r + 28, s.down);
        MemoryUtil.memPutFloat(r + 32, s.u0);
        MemoryUtil.memPutFloat(r + 36, s.u1);
        MemoryUtil.memPutFloat(r + 40, s.v0);
        MemoryUtil.memPutFloat(r + 44, s.v1);
        MemoryUtil.memPutInt(r + 48, g.color());
        MemoryUtil.memPutInt(r + 52, g.shadowColor());
        MemoryUtil.memPutInt(r + 56, (g.style().isItalic() ? 1 : 0) | (bold ? 2 : 0));
        return 4 * (bold ? 2 : 1) * (shadow ? 2 : 1);
    }

    /**
     * A run of glyph records made beforehand (TextPlan) into the consumer at once, after anything still waiting; false
     * when the consumer does not take native glyphs (then the caller draws them one by one).
     */
    public static boolean renderRun(VertexConsumer consumer, Matrix4fc pose, int light, long records, int glyphCount, int vertexCount,
                                    BakedSheetGlyph.GlyphInstance[] glyphs) {
        if (OFF || !(consumer instanceof BufferBuilder b) || !textFormat(b) || !NativeTerrain.active()) {
            return false;
        }
        flush();
        begin(b, pose, light, false);
        long dst = ((VertexAppend) b).cursed$reserve(vertexCount);
        int written = Native.textGlyphs(header, records, glyphCount, dst);
        if (written != vertexCount) {
            throw new IllegalStateException("Glyph vertices: wrote " + written + " of " + vertexCount);
        }
        if (CHECK) {
            java.util.Collections.addAll(pending, glyphs);
            check(dst, written);
            pending.clear();
        }
        builder = null;
        return true;
    }

    /** Writes the run so far. Called before anything else draws, and when the renderer's group ends. */
    public static void flush() {
        if (count == 0) {
            return;
        }
        long dst = ((VertexAppend) builder).cursed$reserve(vertices);
        int written = Native.textGlyphs(header, glyphs, count, dst);
        if (written != vertices) {
            throw new IllegalStateException("Glyph vertices: wrote " + written + " of " + vertices);
        }
        if (CHECK) {
            check(dst, written);
            pending.clear();
        }
        count = 0;
        vertices = 0;
        builder = null;
    }

    private static void check(long ours, int written) {
        if (scratch == null) {
            scratch = new ByteBufferBuilder(1 << 16);
        }
        BufferBuilder game = new BufferBuilder(scratch, PrimitiveTopology.QUADS, format);
        int size = format.getVertexSize();
        for (BakedSheetGlyph.GlyphInstance g : pending) {
            g.render(pose, game, light, flat);
        }
        int differ = 0;
        String first = null;
        MeshData mesh = game.build();
        try {
            ByteBuffer g = mesh.vertexBuffer();
            int n = Math.min(written, mesh.drawState().vertexCount());
            if (n != written || mesh.drawState().vertexCount() != written) {
                first = written + " vertices vs the game's " + mesh.drawState().vertexCount();
                differ++;
            }
            for (int i = 0; i < n * size; i++) {
                byte x = MemoryUtil.memGetByte(ours + i);
                byte y = g.get(g.position() + i);
                if (x != y) {
                    if (first == null) {
                        first = "vertex " + i / size + " byte " + i % size + ": " + (x & 0xFF) + " vs the game's " + (y & 0xFF);
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
                LOGGER.warn("[text] glyphs differ from the game: {} bytes; first {}", differ, first);
            }
        }
        if (checked.sum() % 2000 == 0) {
            LOGGER.info("[text] checked {} runs, {} differ", checked.sum(), mismatched.sum());
        }
    }
}
