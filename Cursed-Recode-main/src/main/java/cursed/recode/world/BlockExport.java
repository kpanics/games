package cursed.recode.world;

import cursed.recode.CursedRecode;
import it.unimi.dsi.fastutil.ints.IntArrayList;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.IdentityHashMap;
import java.util.List;
import java.util.Map;
import java.util.stream.IntStream;
import net.minecraft.client.color.block.BlockColors;
import net.minecraft.client.renderer.block.BlockStateModelSet;
import net.minecraft.client.renderer.block.dispatch.BlockStateModel;
import net.minecraft.client.renderer.block.dispatch.BlockStateModelPart;
import net.minecraft.client.renderer.block.dispatch.SingleVariant;
import net.minecraft.client.renderer.block.dispatch.WeightedVariants;
import net.minecraft.client.renderer.block.dispatch.multipart.MultiPartModel;
import net.minecraft.client.resources.model.geometry.BakedQuad;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.tags.BlockTags;
import net.minecraft.util.Mth;
import net.minecraft.util.random.Weighted;
import net.minecraft.world.level.EmptyBlockGetter;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.CrossCollisionBlock;
import net.minecraft.world.level.block.HalfTransparentBlock;
import net.minecraft.world.level.block.IronBarsBlock;
import net.minecraft.world.level.block.LeavesBlock;
import net.minecraft.world.level.block.MangroveRootsBlock;
import net.minecraft.world.level.block.PowderSnowBlock;
import net.minecraft.world.level.block.RenderShape;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.BooleanProperty;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import net.minecraft.world.phys.shapes.BooleanOp;
import net.minecraft.world.phys.shapes.Shapes;
import net.minecraft.world.phys.shapes.VoxelShape;
import org.lwjgl.system.MemoryUtil;

/**
 * Every block state's rendering facts and the baked models, for the native mesher (native/src/world/blocks.rs reads
 * this layout; its header comment describes it). Made once per resource reload, when the game makes a new
 * SectionCompiler. What the mesher could not reproduce is marked JAVA_ONLY: sections with such blocks stay with the game.
 */
final class BlockExport {
    static final int VERSION = 3;

    static final int AIR = 1;
    static final int MODEL = 2;
    static final int SOLID_RENDER = 4;
    static final int LIGHT_PERMEABLE = 8;
    static final int EMISSIVE = 16;
    static final int COLLISION_FULL = 32;
    static final int LEAVES = 64;
    static final int BARS_TAG = 128;
    static final int MANGROVE_ROOTS = 256;
    static final int HAS_FLUID = 512;
    static final int HAS_BLOCK_ENTITY = 1024;
    static final int JAVA_ONLY = 2048;

    static final int SKIP_NONE = 0;
    static final int SKIP_SAME_BLOCK = 1;
    static final int SKIP_BARS = 2;
    static final int SKIP_LEAVES = 3;
    static final int SKIP_MANGROVE = 4;

    static final int OFFSET_NONE = 0;
    static final int OFFSET_XZ = 1;
    static final int OFFSET_XYZ = 2;

    private static final Direction[] DIRECTIONS = Direction.values();
    /** Where seeds and offsets are checked against the game's own answers. */
    private static final BlockPos[] PROBES = {
            new BlockPos(123, 45, -678), new BlockPos(-1000, 70, 33), new BlockPos(5, -20, 99999), new BlockPos(-77777, 300, -4321),
            new BlockPos(0, 0, 0), new BlockPos(31, -64, 17)
    };
    /** What getSeed may move the position by: none, the lower half of a door or a plant, the other half of a bed. */
    private static final int[][] SEED_OFFSETS = {{0, 0, 0}, {0, -1, 0}, {1, 0, 0}, {-1, 0, 0}, {0, 0, 1}, {0, 0, -1}};

    private final Map<VoxelShape, Integer> shapeIds = new IdentityHashMap<>();
    private final Map<List<AABB>, Integer> shapeByContent = new HashMap<>();
    private final List<VoxelShape> shapes = new ArrayList<>();
    private final Map<BakedQuad, Integer> quadIds = new IdentityHashMap<>();
    private final List<BakedQuad> quads = new ArrayList<>();
    private final Map<BlockStateModelPart, Integer> partIds = new IdentityHashMap<>();
    private final List<BlockStateModelPart> parts = new ArrayList<>();
    private final Map<BlockStateModel, Integer> nodeIds = new IdentityHashMap<>();
    private final IntArrayList tree = new IntArrayList();
    private int javaOnly;

    private BlockExport() {
        this.shapes.add(Shapes.empty());
        this.shapes.add(Shapes.block());
    }

    /** Builds the tables and hands them to the DLL. */
    static void export(BlockStateModelSet models, BlockColors colors, net.minecraft.client.renderer.block.FluidStateModelSet fluidModels) {
        long start = System.nanoTime();
        BlockExport e = new BlockExport();
        int count = Block.BLOCK_STATE_REGISTRY.size();
        Bytes states = new Bytes(count * 96);
        for (int id = 0; id < count; id++) {
            BlockState state = Block.BLOCK_STATE_REGISTRY.byId(id);
            if (state == null) {
                states.i32(AIR | JAVA_ONLY).i32(0).f32(1.0F).i32(-1).i32(0).i32(0).i32(0).i32(0).i32(OFFSET_NONE).f32(0).f32(0);
                for (int d = 0; d < 6; d++) {
                    states.i32(0);
                }
                states.i32(SKIP_NONE).i32(0).i32(-1);
                continue;
            }
            ((StateId) state).cursed$setId(id);
            e.state(states, state, models, colors);
        }
        Bytes all = e.assemble(states, count);
        e.fluids(all, count, fluidModels);
        if (cursed.recode.dev.Dev.on("CURSED_DEV_MESHBENCH")) {
            try {
                java.nio.file.Files.createDirectories(java.nio.file.Path.of("meshbench"));
                byte[] b = new byte[all.length()];
                MemoryUtil.memByteBuffer(all.address(), all.length()).get(b);
                java.nio.file.Files.write(java.nio.file.Path.of("meshbench", "tables.bin"), b);
            } catch (java.io.IOException ex) {
                CursedRecode.LOGGER.warn("[native mesher] dump", ex);
            }
        }
        try {
            cursed.recode.render.Native.worldTables(all.address(), all.length());
        } finally {
            all.free();
            states.free();
        }
        CursedRecode.LOGGER.info("[native mesher] tables: {} states ({} stay with the game), {} quads, {} parts, {} face shapes, {} ms",
                count, e.javaOnly, e.quads.size(), e.parts.size(), e.shapes.size(), (System.nanoTime() - start) / 1_000_000);
    }

    private void state(Bytes out, BlockState state, BlockStateModelSet models, BlockColors colors) {
        Block block = state.getBlock();
        int flags = 0;
        if (state.isAir()) {
            flags |= AIR;
        }
        boolean model = state.getRenderShape() == RenderShape.MODEL;
        if (model) {
            flags |= MODEL;
        }
        if (state.isSolidRender()) {
            flags |= SOLID_RENDER;
        }
        if (state.isLightPermeable()) {
            flags |= LIGHT_PERMEABLE;
        }
        if (state.emissiveRendering()) {
            flags |= EMISSIVE;
        }
        if (state.isCollisionShapeFullBlock(EmptyBlockGetter.INSTANCE, BlockPos.ZERO)) {
            flags |= COLLISION_FULL;
        }
        if (block instanceof LeavesBlock) {
            flags |= LEAVES;
        }
        if (state.is(BlockTags.BARS)) {
            flags |= BARS_TAG;
        }
        if (state.is(Blocks.MANGROVE_ROOTS)) {
            flags |= MANGROVE_ROOTS;
        }
        if (!state.getFluidState().isEmpty()) {
            flags |= HAS_FLUID;
        }
        if (state.hasBlockEntity()) {
            flags |= HAS_BLOCK_ENTITY;
        }

        int[] seed = seedOffset(state);
        int offsetType = offsetType(state);
        int node = -1;
        if (model) {
            node = this.node(models.get(state));
        }
        if (model && (seed == null || offsetType < 0 || node < 0)) {
            flags |= JAVA_ONLY;
            this.javaOnly++;
        }

        out.i32(flags)
                .i32(state.getLightEmission())
                .f32(state.getShadeBrightness(EmptyBlockGetter.INSTANCE, BlockPos.ZERO))
                .i32(BuiltInRegistries.BLOCK.getId(block))
                .i32(colors.getTintSources(state).size());
        if (seed == null) {
            seed = SEED_OFFSETS[0];
        }
        out.i32(seed[0]).i32(seed[1]).i32(seed[2])
                .i32(Math.max(offsetType, 0))
                .f32(block.getMaxHorizontalOffset())
                .f32(block.getMaxVerticalOffset());
        for (Direction d : DIRECTIONS) {
            out.i32(this.shape(state.getFaceOcclusionShape(d)));
        }
        out.i32(skipRule(block)).i32(bars(state)).i32(node);
    }

    private static int skipRule(Block block) {
        if (block instanceof HalfTransparentBlock || block instanceof PowderSnowBlock) {
            return SKIP_SAME_BLOCK;
        }
        if (block instanceof IronBarsBlock) {
            return SKIP_BARS;
        }
        if (block instanceof LeavesBlock) {
            return SKIP_LEAVES;
        }
        if (block instanceof MangroveRootsBlock) {
            return SKIP_MANGROVE;
        }
        return SKIP_NONE;
    }

    /** IronBarsBlock.skipRendering's view of a state: bit d = connected that way, bit 8 + d = has that property. */
    private static int bars(BlockState state) {
        int bits = 0;
        for (Map.Entry<Direction, BooleanProperty> e : CrossCollisionBlock.PROPERTY_BY_DIRECTION.entrySet()) {
            int d = e.getKey().ordinal();
            if (state.hasProperty(e.getValue())) {
                bits |= 1 << (8 + d);
                if (state.getValue(e.getValue())) {
                    bits |= 1 << d;
                }
            }
        }
        return bits;
    }

    /** How getSeed moves the position before Mth.getSeed, checked against the game; null when no simple move fits. */
    private static int[] seedOffset(BlockState state) {
        search:
        for (int[] o : SEED_OFFSETS) {
            for (BlockPos p : PROBES) {
                if (state.getSeed(p) != Mth.getSeed(p.getX() + o[0], p.getY() + o[1], p.getZ() + o[2])) {
                    continue search;
                }
            }
            return o;
        }
        return null;
    }

    /** The offset type (BlockBehaviour.Properties.offsetType), checked against the game; -1 when neither fits. */
    private static int offsetType(BlockState state) {
        if (!state.hasOffsetFunction()) {
            return OFFSET_NONE;
        }
        Block block = state.getBlock();
        for (int type : new int[]{OFFSET_XZ, OFFSET_XYZ}) {
            boolean fits = true;
            for (BlockPos p : PROBES) {
                Vec3 game = state.getOffset(p);
                Vec3 ours = offset(type, block, p);
                if (game.x != ours.x || game.y != ours.y || game.z != ours.z) {
                    fits = false;
                    break;
                }
            }
            if (fits) {
                return type;
            }
        }
        return -1;
    }

    /** BlockBehaviour.Properties.offsetType's functions (native/src/world/mesher.rs Mesher::offset is the same). */
    private static Vec3 offset(int type, Block block, BlockPos pos) {
        long i = Mth.getSeed(pos.getX(), 0, pos.getZ());
        float f = block.getMaxHorizontalOffset();
        double x = Mth.clamp(((float) (i & 15L) / 15.0F - 0.5) * 0.5, -f, f);
        double z = Mth.clamp(((float) (i >> 8 & 15L) / 15.0F - 0.5) * 0.5, -f, f);
        double y = type == OFFSET_XYZ ? ((float) (i >> 4 & 15L) / 15.0F - 1.0) * block.getMaxVerticalOffset() : 0.0;
        return new Vec3(x, y, z);
    }

    /** A face shape's number: 0 Shapes.empty(), 1 Shapes.block(), else one per distinct content. */
    private int shape(VoxelShape s) {
        if (s == Shapes.empty()) {
            return 0;
        }
        if (s == Shapes.block()) {
            return 1;
        }
        Integer id = this.shapeIds.get(s);
        if (id == null) {
            id = this.shapeByContent.computeIfAbsent(s.toAabbs(), k -> {
                this.shapes.add(s);
                return this.shapes.size() - 1;
            });
            this.shapeIds.put(s, id);
        }
        return id;
    }

    /** A model as a node of the tree (blocks.rs: 0 part | 1 n total (weight node)×n | 2 n node×n); -1 if unknown. */
    private int node(BlockStateModel model) {
        if (model instanceof MultiPartModel multi) {
            List<BlockStateModel> selected = multi.shared.selectModels(multi.blockState);
            int[] children = new int[selected.size()];
            for (int i = 0; i < children.length; i++) {
                children[i] = this.node(selected.get(i));
                if (children[i] < 0) {
                    return -1;
                }
            }
            int at = this.tree.size();
            this.tree.add(2);
            this.tree.add(children.length);
            this.tree.addElements(this.tree.size(), children);
            return at;
        }
        Integer known = this.nodeIds.get(model);
        if (known != null) {
            return known;
        }
        int at;
        if (model instanceof SingleVariant single) {
            at = this.tree.size();
            this.tree.add(0);
            this.tree.add(this.part(single.model));
        } else if (model instanceof WeightedVariants weighted) {
            List<Weighted<BlockStateModel>> entries = weighted.list.unwrap();
            int[] children = new int[entries.size()];
            int total = 0;
            for (int i = 0; i < children.length; i++) {
                children[i] = this.node(entries.get(i).value());
                if (children[i] < 0) {
                    return -1;
                }
                total = Math.addExact(total, entries.get(i).weight());
            }
            if (total <= 0) {
                return -1;
            }
            at = this.tree.size();
            this.tree.add(1);
            this.tree.add(children.length);
            this.tree.add(total);
            for (int i = 0; i < children.length; i++) {
                this.tree.add(entries.get(i).weight());
                this.tree.add(children[i]);
            }
        } else {
            return -1;
        }
        this.nodeIds.put(model, at);
        return at;
    }

    private int part(BlockStateModelPart part) {
        Integer id = this.partIds.get(part);
        if (id == null) {
            id = this.parts.size();
            this.parts.add(part);
            this.partIds.put(part, id);
            for (Direction d : DIRECTIONS) {
                part.getQuads(d).forEach(this::quad);
            }
            part.getQuads(null).forEach(this::quad);
        }
        return id;
    }

    private int quad(BakedQuad quad) {
        return this.quadIds.computeIfAbsent(quad, q -> {
            this.quads.add(q);
            return this.quads.size() - 1;
        });
    }

    private Bytes assemble(Bytes states, int count) {
        int n = this.shapes.size();
        byte[] occludes = new byte[n * n];
        IntStream.range(0, n).parallel().forEach(a -> {
            VoxelShape self = this.shapes.get(a);
            for (int b = 0; b < n; b++) {
                occludes[a * n + b] = (byte) (Shapes.joinIsNotEmpty(self, this.shapes.get(b), BooleanOp.ONLY_FIRST) ? 1 : 0);
            }
        });
        Bytes out = new Bytes(states.length() + this.quads.size() * 104 + n * n + (1 << 16));
        out.i32(VERSION).i32(count).i32(this.quads.size()).i32(this.parts.size()).i32(n).i32(this.tree.size());
        out.bytes(occludes);
        for (BakedQuad q : this.quads) {
            for (int v = 0; v < 4; v++) {
                out.f32(q.position(v).x()).f32(q.position(v).y()).f32(q.position(v).z());
            }
            for (int v = 0; v < 4; v++) {
                out.i64(q.packedUV(v));
            }
            BakedQuad.MaterialInfo m = q.materialInfo();
            Direction shade = m.shadeDirectionOverride();
            out.i32(q.direction().ordinal()).i32(m.tintIndex()).i32(shade == null ? -1 : shade.ordinal()).i32(m.lightEmission()).i32(m.layer().ordinal());
        }
        for (BlockStateModelPart p : this.parts) {
            out.i32(p.useAmbientOcclusion() ? 1 : 0);
            List<List<BakedQuad>> lists = new ArrayList<>(7);
            for (Direction d : DIRECTIONS) {
                lists.add(p.getQuads(d));
            }
            lists.add(p.getQuads(null));
            for (List<BakedQuad> l : lists) {
                out.i32(l.size());
            }
            for (List<BakedQuad> l : lists) {
                for (BakedQuad q : l) {
                    out.i32(this.quadIds.get(q));
                }
            }
        }
        for (int i = 0; i < this.tree.size(); i++) {
            out.i32(this.tree.getInt(i));
        }
        out.copy(states);
        return out;
    }

    // ---- fluids (FluidRenderer, FlowingFluid.getFlow) -----------------------------------------------------------------

    static final int F_FALLING = 1;
    static final int F_BLOCKS_FLOW = 2;
    static final int F_LEGACY_SOLID = 4;
    static final int F_ICE = 8;
    /** HalfTransparentBlock or LeavesBlock: a fluid's side next to it shows the overlay texture. */
    static final int F_OVERLAY_NEIGHBOUR = 16;
    /** Bit 8 + d: the face toward direction d is sturdy (SupportType.FULL). */
    static final int F_STURDY = 8;
    /** Bit 16 + d: the fluid's own block hides its face toward d (FluidRenderer.isFaceOccludedBySelf). */
    static final int F_SELF_OCCLUDED = 16;

    /**
     * The fluid part of the export (blocks.rs reads it after the states): the fluid models, how every face shape hides a
     * fluid's face (FluidRenderer.isFaceOccludedByState, as height thresholds), each state's fluid facts, and the
     * game's own trigonometry tables (Mth.SIN, ASIN_TAB, COS_TAB: Java's Math.sin may differ from Rust's in the last bit).
     */
    private void fluids(Bytes out, int count, net.minecraft.client.renderer.block.FluidStateModelSet fluidModels) {
        Map<net.minecraft.client.renderer.block.FluidModel, Integer> modelIds = new IdentityHashMap<>();
        List<net.minecraft.client.renderer.block.FluidModel> fluidModelList = new ArrayList<>();
        Bytes perState = new Bytes(count * 16);
        for (int id = 0; id < count; id++) {
            BlockState state = Block.BLOCK_STATE_REGISTRY.byId(id);
            if (state == null) {
                perState.i32(0).f32(0).i32(-1).i32(0);
                continue;
            }
            net.minecraft.world.level.material.FluidState fluid = state.getFluidState();
            int group = 0;
            int model = -1;
            int flags = 0;
            if (!fluid.isEmpty()) {
                net.minecraft.world.level.material.Fluid type = fluid.getType();
                net.minecraft.world.level.material.Fluid source = type instanceof net.minecraft.world.level.material.FlowingFluid f ? f.getSource() : type;
                group = BuiltInRegistries.FLUID.getId(source) + 1;
                net.minecraft.client.renderer.block.FluidModel m = fluidModels.get(fluid);
                model = modelIds.computeIfAbsent(m, k -> {
                    fluidModelList.add(k);
                    return fluidModelList.size() - 1;
                });
                if (fluid.hasProperty(net.minecraft.world.level.material.FlowingFluid.FALLING) && fluid.getValue(net.minecraft.world.level.material.FlowingFluid.FALLING)) {
                    flags |= F_FALLING;
                }
                for (Direction d : DIRECTIONS) {
                    if (!net.minecraft.client.renderer.block.FluidRenderer.shouldRenderFace(fluid, state, d, net.minecraft.world.level.material.Fluids.EMPTY.defaultFluidState())) {
                        flags |= 1 << (F_SELF_OCCLUDED + d.ordinal());
                    }
                }
            }
            if (state.is(BlockTags.BLOCKS_FLUID_FLOW)) {
                flags |= F_BLOCKS_FLOW;
            }
            if (state.isSolid()) {
                flags |= F_LEGACY_SOLID;
            }
            Block block = state.getBlock();
            if (block instanceof net.minecraft.world.level.block.IceBlock) {
                flags |= F_ICE;
            }
            if (block instanceof HalfTransparentBlock || block instanceof LeavesBlock) {
                flags |= F_OVERLAY_NEIGHBOUR;
            }
            for (Direction d : DIRECTIONS) {
                if (state.isFaceSturdy(EmptyBlockGetter.INSTANCE, BlockPos.ZERO, d)) {
                    flags |= 1 << (F_STURDY + d.ordinal());
                }
            }
            perState.i32(group).f32(fluid.getOwnHeight()).i32(model).i32(flags);
        }
        out.i32(fluidModelList.size());
        for (net.minecraft.client.renderer.block.FluidModel m : fluidModelList) {
            out.i32(m.layer().ordinal());
            sprite(out, m.stillMaterial().sprite());
            sprite(out, m.flowingMaterial().sprite());
            out.i32(m.overlayMaterial() != null ? 1 : 0);
            if (m.overlayMaterial() != null) {
                sprite(out, m.overlayMaterial().sprite());
            } else {
                out.f32(0).f32(0).f32(0).f32(0);
            }
            out.i32(m.tintSource() != null ? 1 : 0);
        }
        // Per face shape: DOWN at the fixed height, UP at full height, and for the four sides the greatest height the
        // shape still hides (a side face of height h is hidden iff h <= it; -1: never).
        for (VoxelShape s : this.shapes) {
            out.i32(fluidOccludes(s, Direction.DOWN, 0.8888889F) ? 1 : 0);
            out.i32(fluidOccludes(s, Direction.UP, 1.0F) ? 1 : 0);
            for (Direction d : new Direction[]{Direction.NORTH, Direction.SOUTH, Direction.WEST, Direction.EAST}) {
                out.f64(sideThreshold(s, d));
            }
        }
        out.copy(perState);
        perState.free();
        float[] sin = net.minecraft.util.Mth.SIN;
        for (float v : sin) {
            out.f32(v);
        }
        for (double v : net.minecraft.util.Mth.ASIN_TAB) {
            out.f64(v);
        }
        for (double v : net.minecraft.util.Mth.COS_TAB) {
            out.f64(v);
        }
    }

    private static void sprite(Bytes out, net.minecraft.client.renderer.texture.TextureAtlasSprite s) {
        out.f32(s.getU0()).f32(s.getU1()).f32(s.getV0()).f32(s.getV1());
    }

    /** FluidRenderer.isFaceOccludedByState with the shape in place of the state's face shape. */
    private static boolean fluidOccludes(VoxelShape shape, Direction direction, float height) {
        if (shape == Shapes.empty()) {
            return false;
        } else if (shape == Shapes.block()) {
            return direction != Direction.UP || height == 1.0F;
        } else {
            return Shapes.blockOccludes(Shapes.box(0.0, 0.0, 0.0, 1.0, height, 1.0), shape, direction);
        }
    }

    /** The greatest fluid side height the shape hides; whether a height hides only changes at the shape's y coords. */
    private static double sideThreshold(VoxelShape shape, Direction direction) {
        if (shape == Shapes.empty()) {
            return -1.0;
        }
        if (shape == Shapes.block()) {
            return 2.0;
        }
        double best = -1.0;
        for (double y : shape.getCoords(net.minecraft.core.Direction.Axis.Y)) {
            if (y > 1.0E-7 && y <= 1.0 && Shapes.blockOccludes(Shapes.box(0.0, 0.0, 0.0, 1.0, y, 1.0), shape, direction)) {
                best = Math.max(best, y);
            }
        }
        return best;
    }

    /** Little-endian bytes in native memory that grow as needed. */
    static final class Bytes {
        private ByteBuffer buf;

        Bytes(int capacity) {
            this.buf = MemoryUtil.memAlloc(Math.max(capacity, 64)).order(ByteOrder.LITTLE_ENDIAN);
        }

        private void ensure(int bytes) {
            if (this.buf.remaining() < bytes) {
                int position = this.buf.position();
                int capacity = Math.max(this.buf.capacity() * 2, position + bytes);
                this.buf = MemoryUtil.memRealloc(this.buf, capacity).order(ByteOrder.LITTLE_ENDIAN);
                this.buf.limit(capacity).position(position);
            }
        }

        Bytes i32(int v) {
            ensure(4);
            this.buf.putInt(v);
            return this;
        }

        Bytes i64(long v) {
            ensure(8);
            this.buf.putLong(v);
            return this;
        }

        Bytes f32(float v) {
            ensure(4);
            this.buf.putFloat(v);
            return this;
        }

        Bytes f64(double v) {
            ensure(8);
            this.buf.putDouble(v);
            return this;
        }

        /** The bytes, then zeros up to a multiple of 4. */
        Bytes bytes(byte[] b) {
            int padded = (b.length + 3) & ~3;
            ensure(padded);
            this.buf.put(b);
            for (int i = b.length; i < padded; i++) {
                this.buf.put((byte) 0);
            }
            return this;
        }

        Bytes copy(Bytes other) {
            int n = other.length();
            ensure(n);
            MemoryUtil.memCopy(other.address(), this.address() + this.buf.position(), n);
            this.buf.position(this.buf.position() + n);
            return this;
        }

        long address() {
            return MemoryUtil.memAddress0(this.buf);
        }

        int length() {
            return this.buf.position();
        }

        void free() {
            MemoryUtil.memFree(this.buf);
        }
    }
}
