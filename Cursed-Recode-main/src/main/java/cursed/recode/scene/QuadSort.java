package cursed.recode.scene;

import com.mojang.blaze3d.vertex.ByteBufferBuilder;
import com.mojang.blaze3d.vertex.CompactVectorArray;
import com.mojang.blaze3d.vertex.MeshData;
import com.mojang.blaze3d.vertex.VertexSorting;
import com.mojang.logging.LogUtils;
import com.mojang.renderpearl.api.pipeline.IndexType;
import com.mojang.renderpearl.api.vertex.VertexFormat;
import com.mojang.renderpearl.api.vertex.VertexFormatElement;
import cursed.recode.dev.Dev;
import cursed.recode.render.Native;
import cursed.recode.render.NativeTerrain;
import java.nio.ByteBuffer;
import java.util.List;
import net.minecraft.client.renderer.StagedVertexBuffer;
import org.lwjgl.system.MemoryUtil;
import org.slf4j.Logger;

/**
 * Translucent quads put in order for the upload by the DLL (native/src/scene/sort.rs) instead of MeshData.SortState
 * (a Java sort with a comparator over every quad, every frame: a seventh of the frame at a crowded spawn, skins'
 * outer layers). Only the game's two sortings, by distance from the camera and by depth in the GUI; the order is the
 * game's to the quad (CURSED_DEV_SORTCHECK compares). CURSED_DEV_JAVASORT keeps the game's. Render thread.
 */
public final class QuadSort {
    private static final Logger LOGGER = LogUtils.getLogger();
    private static final boolean OFF = Dev.on("CURSED_DEV_JAVASORT");
    /** CURSED_DEV_SORTCHECK: the game's order worked out too and compared. */
    public static final boolean CHECKING = Dev.on("CURSED_DEV_SORTCHECK");
    /** What the game's SortState gets instead of centres when the DLL orders the draw (it is not read). */
    public static final CompactVectorArray NONE = new CompactVectorArray(0);
    private static long desc = MemoryUtil.nmemAlloc(16L * 16);
    private static int capacity = 16;
    private static int checked;

    private QuadSort() {
    }

    /** Whether the DLL orders this draw's quads. */
    public static boolean takes(StagedVertexBuffer.Draw draw) {
        return !OFF && NativeTerrain.active() && (draw.quadSorting == VertexSorting.DISTANCE_TO_ORIGIN || draw.quadSorting == VertexSorting.ORTHOGRAPHIC_Z)
                && draw.format.getElement("Position") != null;
    }

    /** The draw's sorted indices into `target` at its position (as SortState.writeSortedIndexBuffer would). */
    public static void write(StagedVertexBuffer.Draw draw, ByteBuffer target, int indexBytes, CompactVectorArray gamePoints) {
        List<ByteBufferBuilder.Result> slices = draw.vertexBufferSlices;
        if (slices.size() > capacity) {
            capacity = Integer.highestOneBit(slices.size()) * 2;
            desc = MemoryUtil.nmemRealloc(desc, 16L * capacity);
        }
        VertexFormat format = draw.format;
        int stride = format.getVertexSize();
        VertexFormatElement position = format.getElement("Position");
        for (int i = 0; i < slices.size(); i++) {
            ByteBuffer b = slices.get(i).byteBuffer();
            MemoryUtil.memPutLong(desc + 16L * i, MemoryUtil.memAddress(b));
            MemoryUtil.memPutInt(desc + 16L * i + 8, b.remaining() / stride);
            MemoryUtil.memPutInt(desc + 16L * i + 12, 0);
        }
        int mode = draw.quadSorting == VertexSorting.ORTHOGRAPHIC_Z ? 1 : 0;
        long dst = MemoryUtil.memAddress(target);
        int quads = Native.quadsSort(desc, slices.size(), stride, position.offset(), mode, dst, indexBytes);
        if (quads != draw.vertexCount / 4) {
            throw new IllegalStateException("Sorted quads: " + quads + " of " + draw.vertexCount / 4);
        }
        if (CHECKING && gamePoints != null && checked < 200) {
            check(draw, dst, indexBytes, gamePoints, quads);
        }
    }

    private static void check(StagedVertexBuffer.Draw draw, long ours, int indexBytes, CompactVectorArray points, int quads) {
        checked++;
        ByteBuffer game = MemoryUtil.memAlloc(quads * 6 * indexBytes);
        try {
            new MeshData.SortState(points, indexBytes == 2 ? IndexType.SHORT : IndexType.INT)
                    .writeSortedIndexBuffer(game, draw.quadSorting);
            game.flip();
            int differ = 0;
            for (int i = 0; i < quads * 6 * indexBytes; i++) {
                if (MemoryUtil.memGetByte(ours + i) != game.get(i)) {
                    differ++;
                }
            }
            if (differ > 0 || checked % 50 == 0) {
                LOGGER.info("[sort] {} quads: {} index bytes differ from the game's ({} checked)", quads, differ, checked);
            }
        } finally {
            MemoryUtil.memFree(game);
        }
    }
}
