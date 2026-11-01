package cursed.recode.engine;

import com.mojang.blaze3d.platform.NativeImage;
import cursed.recode.CursedRecode;
import cursed.recode.render.Native;
import java.util.ArrayList;
import java.util.IdentityHashMap;
import java.util.List;
import java.util.Map;
import net.minecraft.client.renderer.block.BlockStateModelSet;
import net.minecraft.client.renderer.block.dispatch.BlockStateModel;
import net.minecraft.client.renderer.block.dispatch.BlockStateModelPart;
import net.minecraft.client.renderer.texture.SpriteContents;
import net.minecraft.client.renderer.texture.TextureAtlasSprite;
import net.minecraft.client.resources.model.geometry.BakedQuad;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.tags.FluidTags;
import net.minecraft.util.RandomSource;
import net.minecraft.world.level.EmptyBlockGetter;
import net.minecraft.world.level.block.AzaleaBlock;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.CaveVinesBlock;
import net.minecraft.world.level.block.CaveVinesPlantBlock;
import net.minecraft.world.level.block.FlowerBedBlock;
import net.minecraft.world.level.block.HangingRootsBlock;
import net.minecraft.world.level.block.KelpBlock;
import net.minecraft.world.level.block.KelpPlantBlock;
import net.minecraft.world.level.block.LeafLitterBlock;
import net.minecraft.world.level.block.LilyPadBlock;
import net.minecraft.world.level.block.MushroomBlock;
import net.minecraft.world.level.block.NetherWartBlock;
import net.minecraft.world.level.block.SeaPickleBlock;
import net.minecraft.world.level.block.SugarCaneBlock;
import net.minecraft.world.level.block.TallSeagrassBlock;
import net.minecraft.world.level.block.VegetationBlock;
import net.minecraft.world.level.block.VineBlock;
import net.minecraft.world.level.block.LeavesBlock;
import net.minecraft.world.level.block.LiquidBlock;
import net.minecraft.world.level.block.SlabBlock;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.SlabType;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.shapes.Shapes;
import net.minecraft.world.phys.shapes.VoxelShape;
import org.lwjgl.system.MemoryUtil;

/**
 * Every block state as the engine's rays see it (native/src/engine/voxels.rs): its class (full, a half slab, leaves,
 * glass, water, a plant…), the light it gives, the average colour of its textures and the colour of its brightest
 * pixels (what it glows with). Made once per resource reload, after the mesher's block tables.
 */
public final class Materials {
    // Classes (voxels.rs).
    private static final int AIR = 0;
    private static final int FULL = 1;
    private static final int SLAB_LOW = 2;
    private static final int SLAB_TOP = 3;
    private static final int LEAVES = 4;
    private static final int GLASS = 5;
    private static final int WATER = 6;
    private static final int PLANT = 7;
    private static final int PARTIAL = 8;
    private static final int THIN = 9;
    /** Plants that sway in the wind (grass, flowers, crops, vines, kelp): to a ray the same as PLANT. */
    private static final int SWAY = 10;
    /** Metal blocks (gold, iron, diamond, emerald, netherite, copper): a whole block to a ray, and they shine. */
    private static final int SHINY = 11;
    /** Tinted quads (grass, leaves, water) get the plains' colours: the engine's colours are rough anyway. */
    private static final int GRASS = 0x91BD59;
    private static final int FOLIAGE = 0x77AB2F;
    private static final int WATER_TINT = 0x3F76E4;

    private Materials() {
    }

    public static void export(BlockStateModelSet models) {
        long start = System.nanoTime();
        int count = Block.BLOCK_STATE_REGISTRY.size();
        byte[] clouds = clouds();
        long mem = MemoryUtil.nmemAlloc(4 + count * 12L + 4 + 256 * 256);
        Map<TextureAtlasSprite, int[]> sprites = new IdentityHashMap<>();
        RandomSource random = RandomSource.create(42L);
        List<BlockStateModelPart> parts = new ArrayList<>();
        int[] classes = new int[12];
        try {
            MemoryUtil.memPutInt(mem, count);
            for (int id = 0; id < count; id++) {
                BlockState state = Block.BLOCK_STATE_REGISTRY.byId(id);
                long at = mem + 4 + id * 12L;
                if (state == null) {
                    MemoryUtil.memPutInt(at, 0);
                    MemoryUtil.memPutInt(at + 4, 0);
                    MemoryUtil.memPutInt(at + 8, 0);
                    continue;
                }
                int c = classOf(state);
                classes[c]++;
                int emission = Math.clamp(state.getLightEmission(), 0, 15);
                // Colours: the quads' sprites, averaged (rgb sums and count; the bright pixels' too).
                long[] sum = new long[8];
                parts.clear();
                random.setSeed(42L);
                try {
                    BlockStateModel model = models.get(state);
                    model.collectParts(random, parts);
                    for (BlockStateModelPart part : parts) {
                        for (Direction d : Direction.values()) {
                            for (BakedQuad q : part.getQuads(d)) {
                                add(sum, sprites, q, state);
                            }
                        }
                        for (BakedQuad q : part.getQuads(null)) {
                            add(sum, sprites, q, state);
                        }
                    }
                    if (sum[3] == 0 && model.particleMaterial() != null) {
                        add(sum, average(sprites, model.particleMaterial().sprite()), 0xFFFFFF);
                    }
                } catch (RuntimeException e) {
                    // A model that cannot be asked this way: grey.
                }
                if (!state.getFluidState().isEmpty() && sum[3] == 0) {
                    int tint = state.getFluidState().is(FluidTags.WATER) ? WATER_TINT : 0xD06010;
                    sum[0] += tint >> 16 & 255;
                    sum[1] += tint >> 8 & 255;
                    sum[2] += tint & 255;
                    sum[3] = 1;
                }
                int albedo = sum[3] == 0 ? 0x808080 : rgb(sum[0] / sum[3], sum[1] / sum[3], sum[2] / sum[3]);
                int glow = sum[7] == 0 ? albedo : rgb(sum[4] / sum[7], sum[5] / sum[7], sum[6] / sum[7]);
                MemoryUtil.memPutByte(at, (byte) c);
                MemoryUtil.memPutByte(at + 1, (byte) emission);
                MemoryUtil.memPutShort(at + 2, (short) 0);
                MemoryUtil.memPutInt(at + 4, albedo);
                MemoryUtil.memPutInt(at + 8, glow);
            }
            // Then the game's cloud map, for the clouds' shadow (256 x 256, 1 where a cloud is), or 0.
            long at = mem + 4 + count * 12L;
            MemoryUtil.memPutInt(at, clouds != null ? 256 : 0);
            if (clouds != null) {
                for (int i = 0; i < clouds.length; i++) {
                    MemoryUtil.memPutByte(at + 4 + i, clouds[i]);
                }
            }
            Native.engineMaterials(mem, (int) (4 + count * 12L + 4 + (clouds != null ? clouds.length : 0)));
        } finally {
            MemoryUtil.nmemFree(mem);
        }
        CursedRecode.LOGGER.info("[engine] materials of {} block states in {} ms (full {}, metal {}, slabs {}, leaves {}, glass {}, water {}, plants {}, swaying {}, partial {}, thin {})",
                count, (System.nanoTime() - start) / 1_000_000, classes[FULL], classes[SHINY], classes[SLAB_LOW] + classes[SLAB_TOP], classes[LEAVES], classes[GLASS],
                classes[WATER], classes[PLANT], classes[SWAY], classes[PARTIAL], classes[THIN]);
    }

    /**
     * The game's cloud map (textures/environment/clouds.png), 1 where a cloud is (alpha 10 and up, as CloudRenderer
     * reads it), rows of 256; null when it is not 256 x 256 (a resource pack's): then no clouds' shadow.
     */
    private static byte @org.jspecify.annotations.Nullable [] clouds() {
        try (var in = net.minecraft.client.Minecraft.getInstance().getResourceManager()
                .open(net.minecraft.resources.Identifier.withDefaultNamespace("textures/environment/clouds.png"));
             NativeImage image = NativeImage.read(in)) {
            if (image.getWidth() != 256 || image.getHeight() != 256) {
                return null;
            }
            byte[] out = new byte[256 * 256];
            for (int y = 0; y < 256; y++) {
                for (int x = 0; x < 256; x++) {
                    out[y * 256 + x] = (byte) (net.minecraft.util.ARGB.alpha(image.getPixel(x, y)) >= 10 ? 1 : 0);
                }
            }
            return out;
        } catch (Exception e) {
            CursedRecode.LOGGER.warn("[engine] no cloud map, no clouds' shadow", e);
            return null;
        }
    }

    private static int rgb(long r, long g, long b) {
        return (int) (Math.clamp(r, 0, 255) << 16 | Math.clamp(g, 0, 255) << 8 | Math.clamp(b, 0, 255));
    }

    private static void add(long[] sum, Map<TextureAtlasSprite, int[]> sprites, BakedQuad q, BlockState state) {
        BakedQuad.MaterialInfo m = q.materialInfo();
        int tint = 0xFFFFFF;
        if (m.tintIndex() >= 0) {
            tint = state.getBlock() instanceof LeavesBlock ? FOLIAGE : !state.getFluidState().isEmpty() ? WATER_TINT : GRASS;
        }
        add(sum, average(sprites, m.sprite()), tint);
    }

    private static void add(long[] sum, int[] avg, int tint) {
        if (avg[3] == 0) {
            return;
        }
        int tr = tint >> 16 & 255;
        int tg = tint >> 8 & 255;
        int tb = tint & 255;
        sum[0] += avg[0] * tr / 255;
        sum[1] += avg[1] * tg / 255;
        sum[2] += avg[2] * tb / 255;
        sum[3]++;
        if (avg[7] > 0) {
            sum[4] += avg[4] * tr / 255;
            sum[5] += avg[5] * tg / 255;
            sum[6] += avg[6] * tb / 255;
            sum[7]++;
        }
    }

    /** A sprite's average colour of its opaque pixels and of its brightest ones: r g b n, r g b n. */
    private static int[] average(Map<TextureAtlasSprite, int[]> sprites, TextureAtlasSprite sprite) {
        return sprites.computeIfAbsent(sprite, s -> {
            int[] out = new int[8];
            SpriteContents contents = s.contents();
            NativeImage image = contents.originalImage;
            if (image == null) {
                return out;
            }
            int w = image.getWidth();
            int h = Math.min(image.getHeight(), w);
            long r = 0, g = 0, b = 0, n = 0;
            int max = 0;
            int[] px = new int[w * h];
            for (int y = 0; y < h; y++) {
                for (int x = 0; x < w; x++) {
                    int c = image.getPixel(x, y);
                    px[y * w + x] = c;
                    if ((c >>> 24) < 128) {
                        continue;
                    }
                    int cr = c >> 16 & 255, cg = c >> 8 & 255, cb = c & 255;
                    r += cr;
                    g += cg;
                    b += cb;
                    n++;
                    max = Math.max(max, Math.max(cr, Math.max(cg, cb)));
                }
            }
            if (n == 0) {
                return out;
            }
            out[0] = (int) (r / n);
            out[1] = (int) (g / n);
            out[2] = (int) (b / n);
            out[3] = 1;
            long br = 0, bg = 0, bb = 0, bn = 0;
            for (int c : px) {
                if ((c >>> 24) < 128) {
                    continue;
                }
                int cr = c >> 16 & 255, cg = c >> 8 & 255, cb = c & 255;
                if (Math.max(cr, Math.max(cg, cb)) >= max * 0.8) {
                    br += cr;
                    bg += cg;
                    bb += cb;
                    bn++;
                }
            }
            out[4] = (int) (br / bn);
            out[5] = (int) (bg / bn);
            out[6] = (int) (bb / bn);
            out[7] = 1;
            return out;
        });
    }

    /** Plants the wind moves: what grows soft from the ground or hangs, not lily pads, petals, mushrooms or bushes. */
    private static boolean sways(Block block) {
        if (block instanceof VegetationBlock) {
            return !(block instanceof LilyPadBlock || block instanceof FlowerBedBlock || block instanceof LeafLitterBlock
                    || block instanceof MushroomBlock || block instanceof AzaleaBlock || block instanceof SeaPickleBlock
                    || block instanceof NetherWartBlock);
        }
        return block instanceof VineBlock || block instanceof SugarCaneBlock || block instanceof KelpBlock || block instanceof KelpPlantBlock
                || block instanceof TallSeagrassBlock || block instanceof CaveVinesBlock || block instanceof CaveVinesPlantBlock
                || block instanceof HangingRootsBlock;
    }

    /** Blocks of metal and gems, and copper before it weathers. */
    private static boolean shiny(Block block) {
        String path = net.minecraft.core.registries.BuiltInRegistries.BLOCK.getKey(block).getPath();
        return path.matches("(gold|iron|diamond|emerald|netherite)_block") || path.matches("(waxed_)?(copper_block|cut_copper|chiseled_copper)");
    }

    private static int classOf(BlockState state) {
        if (state.isAir()) {
            return AIR;
        }
        Block block = state.getBlock();
        if (block instanceof LiquidBlock) {
            return state.getFluidState().is(FluidTags.WATER) ? WATER : FULL;
        }
        if (block instanceof LeavesBlock) {
            return LEAVES;
        }
        if (sways(block)) {
            return SWAY;
        }
        if (block instanceof SlabBlock) {
            SlabType type = state.getValue(SlabBlock.TYPE);
            return type == SlabType.BOTTOM ? SLAB_LOW : type == SlabType.TOP ? SLAB_TOP : FULL;
        }
        if (state.isSolidRender()) {
            return shiny(block) ? SHINY : FULL;
        }
        VoxelShape shape;
        try {
            shape = state.getShape(EmptyBlockGetter.INSTANCE, BlockPos.ZERO);
        } catch (RuntimeException e) {
            return PARTIAL;
        }
        if (shape.isEmpty()) {
            return PLANT;
        }
        double volume = 0;
        for (AABB box : shape.toAabbs()) {
            volume += box.getXsize() * box.getYsize() * box.getZsize();
        }
        if (shape == Shapes.block() || volume > 0.999) {
            // A whole block that is not drawn solid: glass, ice, slime, honey.
            return GLASS;
        }
        if (state.getCollisionShape(EmptyBlockGetter.INSTANCE, BlockPos.ZERO).isEmpty()) {
            return PLANT;
        }
        return volume >= 0.6 ? PARTIAL : volume >= 0.1 ? THIN : PLANT;
    }
}
