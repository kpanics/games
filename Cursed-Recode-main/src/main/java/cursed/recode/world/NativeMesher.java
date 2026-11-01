package cursed.recode.world;

import com.mojang.blaze3d.vertex.BufferBuilder;
import com.mojang.blaze3d.vertex.MeshData;
import com.mojang.blaze3d.vertex.VertexSorting;
import com.mojang.logging.LogUtils;
import com.mojang.renderpearl.api.pipeline.PrimitiveTopology;
import cursed.recode.dev.Dev;
import cursed.recode.render.Native;
import cursed.recode.render.NativeTerrain;
import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.EnumMap;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.atomic.AtomicLong;
import java.util.concurrent.atomic.LongAdder;
import net.minecraft.client.color.block.BlockColors;
import net.minecraft.client.color.block.BlockTintSource;
import net.minecraft.client.renderer.SectionBufferBuilderPack;
import net.minecraft.client.renderer.block.BlockStateModelSet;
import net.minecraft.client.renderer.block.FluidRenderer;
import net.minecraft.client.renderer.block.FluidStateModelSet;
import net.minecraft.client.renderer.chunk.ChunkSectionLayer;
import net.minecraft.client.renderer.chunk.RenderSectionRegion;
import net.minecraft.client.renderer.chunk.SectionCompiler;
import net.minecraft.client.renderer.chunk.SectionCopy;
import net.minecraft.client.renderer.chunk.VisibilitySet;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.SectionPos;
import net.minecraft.world.level.CardinalLighting;
import net.minecraft.world.level.LightLayer;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.DataLayer;
import net.minecraft.world.level.chunk.PalettedContainer;
import net.minecraft.world.level.lighting.DataLayerStorageMap;
import net.minecraft.world.level.lighting.LayerLightEventListener;
import net.minecraft.world.level.lighting.LevelLightEngine;
import net.minecraft.world.level.lighting.LightEngine;
import net.minecraft.world.level.lighting.SkyLightSectionStorage;
import org.jspecify.annotations.Nullable;
import org.lwjgl.system.MemoryUtil;
import org.slf4j.Logger;

/**
 * The game's SectionCompiler.compile with the blocks meshed in the DLL (native/src/world/mesher.rs): Java copies the
 * section's block states and light (and two blocks around it), the DLL makes the vertices and the visibility, and
 * Java does what stays with the game — fluids (FluidRenderer, into the same layers), block entities, the translucent
 * sort. The vertices are the game's, bit for bit: CURSED_DEV_MESHCHECK compiles every section both ways and compares.
 */
public final class NativeMesher {
    private static final Logger LOGGER = LogUtils.getLogger();
    private static final boolean CHECK = Dev.on("CURSED_DEV_MESHCHECK");
    private static final boolean OFF = Dev.on("CURSED_DEV_NOMESHER");
    private static final boolean BENCH = Dev.on("CURSED_DEV_BENCH");
    /** CURSED_DEV_MESHBENCH: the game's compile also runs (and is thrown away) for every section, to time both. */
    private static final boolean RACE = Dev.on("CURSED_DEV_MESHBENCH");
    private static final ThreadLocal<Job> JOB = ThreadLocal.withInitial(Job::new);
    /** The dev check runs the game's own compile: the mixin stands aside on that thread meanwhile. */
    private static final ThreadLocal<Boolean> GAME = ThreadLocal.withInitial(() -> false);
    private static final ThreadLocal<SectionBufferBuilderPack> CHECK_PACK = ThreadLocal.withInitial(SectionBufferBuilderPack::new);
    private static final Direction[] DIRECTIONS = Direction.values();
    private static final ChunkSectionLayer[] LAYERS = ChunkSectionLayer.values();

    private static volatile boolean ready;
    private static volatile @Nullable BlockStateModelSet exportedModels;
    private static volatile @Nullable BlockColors exportedColors;
    private static volatile @Nullable FluidStateModelSet exportedFluids;
    private static int airId;

    private static final LongAdder sections = new LongAdder();
    private static final LongAdder nanos = new LongAdder();
    private static final LongAdder fallbacks = new LongAdder();
    private static final AtomicLong lastReport = new AtomicLong();
    private static final LongAdder checked = new LongAdder();
    private static final LongAdder mismatched = new LongAdder();
    private static final LongAdder gameNanos = new LongAdder();
    private static final LongAdder fillNanos = new LongAdder();
    private static final LongAdder nativeNanos = new LongAdder();
    private static final LongAdder tintNanos = new LongAdder();
    private static final LongAdder fluidNanos = new LongAdder();
    private static final LongAdder copyNanos = new LongAdder();
    private static final LongAdder buildNanos = new LongAdder();
    private static final LongAdder tintCalls = new LongAdder();

    private NativeMesher() {
    }

    /** A new SectionCompiler (render thread): tables for its models, once per resource reload. */
    public static synchronized void prepare(BlockStateModelSet models, BlockColors colors, FluidStateModelSet fluidModels) {
        if (OFF || !NativeTerrain.active()) {
            ready = false;
            return;
        }
        if (ready && models == exportedModels && colors == exportedColors && fluidModels == exportedFluids) {
            return;
        }
        ready = false;
        BlockExport.export(models, colors, fluidModels);
        cursed.recode.engine.Materials.export(models);
        exportedFluids = fluidModels;
        airId = ((StateId) Blocks.AIR.defaultBlockState()).cursed$id();
        exportedModels = models;
        exportedColors = colors;
        ready = true;
    }

    private static boolean warmed;

    /**
     * The main menu is up and the game's resources are loaded: the tables (and the engine's materials) are made now, on a
     * thread of their own, for the game's current models — not when the first world opens, where they held its way in
     * still for some 0.6 s. The world's own SectionCompiler finds them made (prepare: the same models), or waits for
     * them if it comes first.
     */
    public static void warm() {
        if (warmed || OFF || !NativeTerrain.active()) {
            return;
        }
        warmed = true;
        net.minecraft.client.Minecraft mc = net.minecraft.client.Minecraft.getInstance();
        var mm = mc.getModelManager();
        BlockStateModelSet models = mm.getBlockStateModelSet();
        FluidStateModelSet fluids = mm.getFluidStateModelSet();
        BlockColors colors = mc.getBlockColors();
        Thread t = new Thread(() -> {
            try {
                prepare(models, colors, fluids);
            } catch (Throwable e) {
                LOGGER.warn("[native mesher] the tables made ahead failed (made when a world opens)", e);
            }
        }, "Cursed mesher tables");
        t.setDaemon(true);
        t.setPriority(Thread.MIN_PRIORITY);
        t.start();
    }

    /** The section natively, or null when the game should compile it (worker threads). */
    public static SectionCompiler.@Nullable Results compile(SectionCompiler compiler, boolean ambientOcclusion, boolean cutoutLeaves,
                                                            BlockStateModelSet models, FluidStateModelSet fluidModels, BlockColors colors,
                                                            SectionPos sectionPos, RenderSectionRegion region, VertexSorting sorting,
                                                            SectionBufferBuilderPack builders) {
        if (!ready || GAME.get() || models != exportedModels || colors != exportedColors || fluidModels != exportedFluids) {
            return null;
        }
        SectionCopy center = region.sections[13];
        if (center.debug) {
            return null;
        }
        long start = System.nanoTime();
        SectionCompiler.Results results;
        if (center.section == null) {
            Native.engineAir(sectionPos.minBlockX(), sectionPos.minBlockY(), sectionPos.minBlockZ());
            // Only air: what the game's compile makes of it.
            results = new SectionCompiler.Results();
            results.visibilitySet.setAll(true);
        } else {
            results = JOB.get().compile(ambientOcclusion, cutoutLeaves, fluidModels, colors, sectionPos, region, sorting, builders);
            if (results == null) {
                fallbacks.increment();
                return null;
            }
        }
        sections.increment();
        nanos.add(System.nanoTime() - start);
        if (CHECK) {
            check(compiler, sectionPos, region, sorting, results);
        } else if (RACE) {
            race(compiler, ambientOcclusion, cutoutLeaves, models, fluidModels, colors, sectionPos, region, sorting);
        }
        report();
        return results;
    }

    private record Sample(SectionCompiler compiler, boolean ao, boolean cutoutLeaves, BlockStateModelSet models, FluidStateModelSet fluids,
                          BlockColors colors, SectionPos pos, RenderSectionRegion region, VertexSorting sorting) {
    }

    private static final List<Sample> SAMPLES = new ArrayList<>();
    private static boolean raced;

    /**
     * CURSED_DEV_MESHBENCH: keeps the first 300 sections, then meshes them all on one thread, ours and the game's in
     * turns, five rounds — clean numbers, without twelve workers fighting over the cores.
     */
    private static void race(SectionCompiler compiler, boolean ao, boolean cutoutLeaves, BlockStateModelSet models, FluidStateModelSet fluids,
                             BlockColors colors, SectionPos pos, RenderSectionRegion region, VertexSorting sorting) {
        List<Sample> run;
        synchronized (SAMPLES) {
            if (raced) {
                return;
            }
            SAMPLES.add(new Sample(compiler, ao, cutoutLeaves, models, fluids, colors, pos, region, sorting));
            if (SAMPLES.size() < 300) {
                return;
            }
            raced = true;
            run = new ArrayList<>(SAMPLES);
            SAMPLES.clear();
        }
        Thread t = new Thread(() -> {
            try {
                Thread.sleep(15_000);
            } catch (InterruptedException e) {
                return;
            }
            SectionBufferBuilderPack pack = new SectionBufferBuilderPack();
            long[] ours = new long[2];
            tintNanos.reset();
            tintCalls.reset();
            fluidNanos.reset();
            copyNanos.reset();
            buildNanos.reset();
            long[] game = new long[2];
            for (int round = 0; round < 6; round++) {
                for (Sample x : run) {
                    long a = System.nanoTime();
                    long f0 = fillNanos.sum();
                    long n0 = nativeNanos.sum();
                    SectionCompiler.Results r = compile(x.compiler, x.ao, x.cutoutLeaves, x.models, x.fluids, x.colors, x.pos, x.region, x.sorting, pack);
                    long b = System.nanoTime();
                    if (r != null) {
                        r.release();
                    }
                    GAME.set(true);
                    long c = System.nanoTime();
                    try {
                        x.compiler.compile(x.pos, x.region, x.sorting, pack).release();
                    } finally {
                        GAME.set(false);
                    }
                    long d = System.nanoTime();
                    if (round > 0) {
                        ours[0] += b - a;
                        game[0] += d - c;
                        ours[1] += nativeNanos.sum() - n0;
                        game[1] += fillNanos.sum() - f0;
                    }
                }
            }
            long n = run.size() * 5L;
            LOGGER.info("[mesher race] {} sections x5 on one thread: ours {} us (copying {} us, the DLL {} us, of it tints {} us in {} calls), the game {} us",
                    run.size(), ours[0] / n / 1000.0, game[1] / n / 1000.0, ours[1] / n / 1000.0, tintNanos.sum() / (n + run.size()) / 1000.0,
                    tintCalls.sum() / (n + run.size()), game[0] / n / 1000.0);
            long m = n + run.size();
            LOGGER.info("[mesher race] the rest per section: fluids {} us, into the builders {} us, build + translucent sort {} us",
                    fluidNanos.sum() / m / 1000.0, copyNanos.sum() / m / 1000.0, buildNanos.sum() / m / 1000.0);
        }, "Cursed mesher race");
        t.setDaemon(true);
        t.start();
    }

    private static final java.util.concurrent.atomic.AtomicInteger dumped = new java.util.concurrent.atomic.AtomicInteger();

    /** CURSED_DEV_MESHBENCH: the first 64 sections' inputs, for native/examples/meshbench.rs (run/meshbench/). */
    private static void dump(long mem, int ox, int oy, int oz, int flags) {
        int n = dumped.getAndIncrement();
        if (n >= 64) {
            return;
        }
        try {
            java.nio.file.Path dir = java.nio.file.Path.of("meshbench");
            java.nio.file.Files.createDirectories(dir);
            ByteBuffer b = ByteBuffer.allocate(16 + Job.OUT).order(java.nio.ByteOrder.LITTLE_ENDIAN);
            b.putInt(ox).putInt(oy).putInt(oz).putInt(flags);
            for (int i = 0; i < Job.OUT; i++) {
                b.put(MemoryUtil.memGetByte(mem + i));
            }
            java.nio.file.Files.write(dir.resolve("section" + n + ".bin"), b.array());
        } catch (java.io.IOException e) {
            LOGGER.warn("[mesher] dump", e);
        }
    }

    private static void report() {
        if (!BENCH && !CHECK) {
            return;
        }
        long now = System.nanoTime();
        long last = lastReport.get();
        if (now - last < 10_000_000_000L || !lastReport.compareAndSet(last, now)) {
            return;
        }
        long n = sections.sumThenReset();
        long ns = nanos.sumThenReset();
        long f = fallbacks.sumThenReset();
        if (n == 0 && f == 0) {
            return;
        }
        long fill = fillNanos.sumThenReset();
        long nat = nativeNanos.sumThenReset();
        if (n > 0) {
            LOGGER.info("[mesher] of which copying the blocks and light {} us, the DLL {} us", fill / n / 1000, nat / n / 1000);
        }
        if (CHECK || RACE) {
            long c = checked.sum();
            long g = gameNanos.sumThenReset();
            LOGGER.info("[mesher] {} sections, {} us each (the game: {} us), {} stayed with the game; checked {}, mismatched {}",
                    n, n == 0 ? 0 : ns / n / 1000, n == 0 ? 0 : g / n / 1000, f, c, mismatched.sum());
        } else {
            LOGGER.info("[mesher] {} sections, {} us each, {} stayed with the game", n, n == 0 ? 0 : ns / n / 1000, f);
        }
    }

    private static VisibilitySet visibility(long bits) {
        VisibilitySet v = new VisibilitySet();
        if (bits == -1L) {
            v.setAll(true);
            return v;
        }
        for (int a = 0; a < 6; a++) {
            for (int b = 0; b < 6; b++) {
                if ((bits >>> (a + b * 6) & 1L) != 0) {
                    v.set(DIRECTIONS[a], DIRECTIONS[b], true);
                }
            }
        }
        return v;
    }

    private static BufferBuilder begin(Map<ChunkSectionLayer, BufferBuilder> started, SectionBufferBuilderPack builders, ChunkSectionLayer layer) {
        BufferBuilder b = started.get(layer);
        if (b == null) {
            b = new BufferBuilder(builders.buffer(layer), PrimitiveTopology.QUADS, layer.vertexFormat());
            started.put(layer, b);
        }
        return b;
    }

    /** One worker thread's input and output memory for the DLL, and what the tint callback needs. */
    static final class Job {
        static final int STATES = 0;
        static final int LIGHT = 32000;
        static final int CARDINAL = 40000;
        static final int OUT = 40064;
        static final int FACINGS = OUT + 32;
        static final int POSITIONS = OUT + 192;

        private final long mem;
        private final BlockPos.MutableBlockPos pos = new BlockPos.MutableBlockPos();
        private @Nullable RenderSectionRegion region;
        private @Nullable BlockColors colors;
        private @Nullable FluidStateModelSet fluidModels;
        /** The tint index the DLL asks for a fluid's color (mesher.rs FLUID_TINT). */
        static final int FLUID_TINT = 1000;
        private int ox;
        private int oy;
        private int oz;
        /** The light source being copied: a section's nibbles, or a constant when null; bottom = only its y 0. */
        private byte @Nullable [] srcData;
        private int srcDefault;
        private boolean srcBottom;

        Job() {
            this.mem = MemoryUtil.nmemAlloc(Native.worldJobSize());
        }

        /** Called by the DLL for a tinted quad's color (ModelBlockRenderer.computeTintColor). */
        @SuppressWarnings("unused")
        int tint(int local, int stateId, int index) {
            BlockState state = Block.BLOCK_STATE_REGISTRY.byId(stateId);
            if (index == FLUID_TINT) {
                // FluidRenderer: the fluid model's tint source, asked with the block state.
                BlockTintSource fluidTint = this.fluidModels.get(state.getFluidState()).tintSource();
                this.pos.set(this.ox + (local & 15), this.oy + (local >> 4 & 15), this.oz + (local >> 8 & 15));
                return fluidTint == null ? -1 : fluidTint.colorInWorld(state, this.region, this.pos);
            }
            List<BlockTintSource> sources = this.colors.getTintSources(state);
            if (index >= sources.size()) {
                return -1;
            }
            BlockTintSource source = sources.get(index);
            if (source == null) {
                return -1;
            }
            this.pos.set(this.ox + (local & 15), this.oy + (local >> 4 & 15), this.oz + (local >> 8 & 15));
            if (!RACE) {
                return source.colorInWorld(state, this.region, this.pos);
            }
            long t = System.nanoTime();
            int c = source.colorInWorld(state, this.region, this.pos);
            tintNanos.add(System.nanoTime() - t);
            tintCalls.increment();
            return c;
        }

        SectionCompiler.@Nullable Results compile(boolean ambientOcclusion, boolean cutoutLeaves, FluidStateModelSet fluidModels, BlockColors colors,
                                                  SectionPos sectionPos, RenderSectionRegion region, VertexSorting sorting, SectionBufferBuilderPack builders) {
            int ox = sectionPos.minBlockX();
            int oy = sectionPos.minBlockY();
            int oz = sectionPos.minBlockZ();
            long t0 = System.nanoTime();
            this.fill(region, sectionPos);
            fillNanos.add(System.nanoTime() - t0);
            CardinalLighting c = region.cardinalLighting();
            long cl = this.mem + CARDINAL;
            for (int d = 0; d < 6; d++) {
                MemoryUtil.memPutFloat(cl + d * 4L, c.byFace(DIRECTIONS[d]));
            }
            if (CHECK) {
                this.verifyInput(region, ox, oy, oz);
            }
            this.region = region;
            this.colors = colors;
            this.fluidModels = fluidModels;
            this.ox = ox;
            this.oy = oy;
            this.oz = oz;
            int r;
            long t1 = System.nanoTime();
            try {
                r = Native.worldMesh(this.mem, ox, oy, oz, (ambientOcclusion ? 1 : 0) | (cutoutLeaves ? 2 : 0), this);
            } finally {
                this.region = null;
                this.colors = null;
            }
            nativeNanos.add(System.nanoTime() - t1);
            if (RACE) {
                dump(this.mem, ox, oy, oz, (ambientOcclusion ? 1 : 0) | (cutoutLeaves ? 2 : 0));
            }
            if (r != 0) {
                return null;
            }
            long out = this.mem + OUT;
            int blockEntities = MemoryUtil.memGetInt(out + 12);
            int fluids = MemoryUtil.memGetInt(out + 16);
            long visibility = MemoryUtil.memGetLong(out + 24);
            SectionCompiler.Results results = new SectionCompiler.Results();
            BlockPos.MutableBlockPos p = this.pos;
            for (int i = 0; i < blockEntities; i++) {
                int local = MemoryUtil.memGetInt(this.mem + POSITIONS + i * 4L);
                p.set(ox + (local & 15), oy + (local >> 4 & 15), oz + (local >> 8 & 15));
                BlockEntity entity = region.getBlockEntity(p);
                if (entity != null) {
                    results.blockEntities.add(entity);
                }
            }
            Map<ChunkSectionLayer, BufferBuilder> started = new EnumMap<>(ChunkSectionLayer.class);
            long tf = System.nanoTime();
            if (fluids > 0) {
                FluidRenderer fluidRenderer = new FluidRenderer(fluidModels);
                FluidRenderer.Output output = layer -> begin(started, builders, layer);
                for (int i = 0; i < fluids; i++) {
                    int local = MemoryUtil.memGetInt(this.mem + POSITIONS + (blockEntities + i) * 4L);
                    p.set(ox + (local & 15), oy + (local >> 4 & 15), oz + (local >> 8 & 15));
                    BlockState state = region.getBlockState(p);
                    fluidRenderer.tesselate(region, p, output, state, state.getFluidState());
                }
            }
            long tc = System.nanoTime();
            fluidNanos.add(tc - tf);
            int[] facings = new int[3 * FacingHolder.COUNT];
            for (int l = 0; l < 3; l++) {
                // The game's fluid quads come first in a layer: they count as unculled.
                BufferBuilder fluid = started.get(LAYERS[l]);
                int fluidQuads = fluid == null ? 0 : ((VertexAppend) fluid).cursed$vertices() / 4;
                for (int k = 0; k < FacingHolder.COUNT; k++) {
                    facings[l * FacingHolder.COUNT + k] = MemoryUtil.memGetInt(this.mem + FACINGS + (l * FacingHolder.COUNT + k) * 4L);
                }
                facings[l * FacingHolder.COUNT] += fluidQuads;
                int count = MemoryUtil.memGetInt(out + l * 4L);
                if (count > 0) {
                    long dst = ((VertexAppend) begin(started, builders, LAYERS[l])).cursed$reserve(count);
                    Native.worldCopy(l, dst);
                }
            }
            ((FacingHolder) (Object) results).cursed$setFacings(facings);
            long tb = System.nanoTime();
            copyNanos.add(tb - tc);
            for (Map.Entry<ChunkSectionLayer, BufferBuilder> e : started.entrySet()) {
                ChunkSectionLayer layer = e.getKey();
                MeshData mesh = e.getValue().build();
                if (mesh != null) {
                    if (layer == ChunkSectionLayer.TRANSLUCENT) {
                        results.transparencyState = mesh.sortQuads(builders.buffer(layer), sorting);
                    }
                    results.renderedLayers.put(layer, mesh);
                }
            }
            buildNanos.add(System.nanoTime() - tb);
            results.visibilitySet = visibility(visibility);
            return results;
        }

        /** Dev check: what was copied is what the game's region answers. */
        private void verifyInput(RenderSectionRegion region, int ox, int oy, int oz) {
            BlockPos.MutableBlockPos p = new BlockPos.MutableBlockPos();
            int states = 0;
            int lights = 0;
            String first = null;
            for (int y = -2; y < 18; y++) {
                for (int z = -2; z < 18; z++) {
                    for (int x = -2; x < 18; x++) {
                        p.set(ox + x, oy + y, oz + z);
                        BlockState game = region.getBlockState(p);
                        int id = MemoryUtil.memGetInt(this.mem + STATES + at(x, y, z) * 4L);
                        if (((StateId) game).cursed$id() != id) {
                            states++;
                            if (first == null) {
                                first = "state at " + x + "," + y + "," + z + ": " + Block.BLOCK_STATE_REGISTRY.byId(id) + " vs the game's " + game;
                            }
                        }
                        int light = MemoryUtil.memGetByte(this.mem + LIGHT + at(x, y, z)) & 0xFF;
                        int gameLight = region.getBrightness(LightLayer.BLOCK, p) | region.getBrightness(LightLayer.SKY, p) << 4;
                        if (light != gameLight) {
                            lights++;
                            if (first == null) {
                                first = "light at " + x + "," + y + "," + z + ": " + Integer.toHexString(light) + " vs the game's " + Integer.toHexString(gameLight);
                            }
                        }
                    }
                }
            }
            if (states + lights > 0) {
                LOGGER.warn("[mesher] input differs: {} states, {} lights; first {}", states, lights, first);
            }
        }

        private static int at(int x, int y, int z) {
            return (x + 2) + (z + 2) * 20 + (y + 2) * 400;
        }

        /** The block states and light of local -2..17 (the section and two blocks around it). */
        private void fill(RenderSectionRegion region, SectionPos sectionPos) {
            LevelLightEngine engine = region.getLightEngine();
            LayerLightEventListener blockLight = engine.getLayerListener(LightLayer.BLOCK);
            LayerLightEventListener skyLight = engine.getLayerListener(LightLayer.SKY);
            long states = this.mem + STATES;
            long light = this.mem + LIGHT;
            for (int dz = -1; dz <= 1; dz++) {
                int z0 = dz < 0 ? -2 : dz == 0 ? 0 : 16;
                int z1 = dz < 0 ? -1 : dz == 0 ? 15 : 17;
                for (int dy = -1; dy <= 1; dy++) {
                    int y0 = dy < 0 ? -2 : dy == 0 ? 0 : 16;
                    int y1 = dy < 0 ? -1 : dy == 0 ? 15 : 17;
                    for (int dx = -1; dx <= 1; dx++) {
                        int x0 = dx < 0 ? -2 : dx == 0 ? 0 : 16;
                        int x1 = dx < 0 ? -1 : dx == 0 ? 15 : 17;
                        PalettedContainer<BlockState> container = region.sections[(dx + 1) + (dy + 1) * 3 + (dz + 1) * 9].section;
                        if (dx == 0 && dy == 0 && dz == 0 && container != null) {
                            this.center(container, states);
                        } else for (int y = y0; y <= y1; y++) {
                            for (int z = z0; z <= z1; z++) {
                                for (int x = x0; x <= x1; x++) {
                                    int id = container == null ? airId : ((StateId) container.get(x & 15, y & 15, z & 15)).cursed$id();
                                    MemoryUtil.memPutInt(states + at(x, y, z) * 4L, id);
                                }
                            }
                        }
                        long key = SectionPos.asLong(sectionPos.x() + dx, sectionPos.y() + dy, sectionPos.z() + dz);
                        this.blockSource(blockLight, key);
                        for (int y = y0; y <= y1; y++) {
                            for (int z = z0; z <= z1; z++) {
                                for (int x = x0; x <= x1; x++) {
                                    MemoryUtil.memPutByte(light + at(x, y, z), (byte) this.nibble(x & 15, y & 15, z & 15));
                                }
                            }
                        }
                        this.skySource(skyLight, key);
                        for (int y = y0; y <= y1; y++) {
                            for (int z = z0; z <= z1; z++) {
                                for (int x = x0; x <= x1; x++) {
                                    long a = light + at(x, y, z);
                                    MemoryUtil.memPutByte(a, (byte) (MemoryUtil.memGetByte(a) | this.nibble(x & 15, y & 15, z & 15) << 4));
                                }
                            }
                        }
                    }
                }
            }
        }

        private final int[] raw = new int[4096];
        private final int[] paletteIds = new int[256];

        /** The section's own 4096 blocks: the packed palette indices unpacked at once, then the palette mapped. */
        private void center(PalettedContainer<BlockState> container, long states) {
            var data = container.data;
            int[] raw = this.raw;
            data.storage().unpack(raw);
            var palette = data.palette();
            if (!(palette instanceof net.minecraft.world.level.chunk.GlobalPalette)) {
                int n = palette.getSize();
                int[] ids = n <= this.paletteIds.length ? this.paletteIds : new int[n];
                for (int i = 0; i < n; i++) {
                    ids[i] = ((StateId) palette.valueFor(i)).cursed$id();
                }
                for (int i = 0; i < 4096; i++) {
                    raw[i] = ids[raw[i]];
                }
            }
            // Palette order is y << 8 | z << 4 | x (Strategy.getIndex); the job's is at(x, y, z).
            for (int y = 0; y < 16; y++) {
                for (int z = 0; z < 16; z++) {
                    long dst = states + at(0, y, z) * 4L;
                    int src = y << 8 | z << 4;
                    for (int x = 0; x < 16; x++) {
                        MemoryUtil.memPutInt(dst + x * 4L, raw[src + x]);
                    }
                }
            }
        }

        private int nibble(int x, int y, int z) {
            byte[] data = this.srcData;
            if (data == null) {
                return this.srcDefault;
            }
            int index = (this.srcBottom ? 0 : y) << 8 | z << 4 | x;
            return data[index >> 1] >> ((index & 1) << 2) & 15;
        }

        private void source(@Nullable DataLayer layer, boolean bottom) {
            if (layer == null) {
                this.srcData = null;
                this.srcDefault = 0;
                this.srcBottom = false;
            } else {
                this.srcData = layer.data;
                this.srcDefault = layer.defaultValue;
                this.srcBottom = bottom;
            }
        }

        /** BlockLightSectionStorage.getLightValue for a whole section. */
        private void blockSource(LayerLightEventListener listener, long key) {
            if (!(listener instanceof LightEngine<?, ?> engine)) {
                this.source(null, false);
                return;
            }
            DataLayerStorageMap<?> map = engine.storage.visibleSectionData;
            this.source(map.getLayer(key), false);
        }

        /** SkyLightSectionStorage.getLightValue(node, false) for a whole section. */
        private void skySource(LayerLightEventListener listener, long key) {
            if (!(listener instanceof LightEngine<?, ?> engine)) {
                this.source(null, false);
                return;
            }
            SkyLightSectionStorage.SkyDataLayerStorageMap map = (SkyLightSectionStorage.SkyDataLayerStorageMap) (Object) engine.storage.visibleSectionData;
            long i = key;
            int j = SectionPos.y(i);
            int k = map.topSections.get(SectionPos.getZeroNode(i));
            if (k != map.currentLowestY && j < k) {
                DataLayer layer = map.getLayer(i);
                boolean bottom = false;
                while (layer == null) {
                    bottom = true;
                    if (++j >= k) {
                        this.srcData = null;
                        this.srcDefault = 15;
                        this.srcBottom = false;
                        return;
                    }
                    i = SectionPos.offset(i, Direction.UP);
                    layer = map.getLayer(i);
                }
                this.source(layer, bottom);
            } else {
                this.srcData = null;
                this.srcDefault = 15;
                this.srcBottom = false;
            }
        }
    }

    // ---- the dev check: the game's compile next to ours --------------------------------------------------------------

    private static void check(SectionCompiler compiler, SectionPos sectionPos, RenderSectionRegion region, VertexSorting sorting, SectionCompiler.Results ours) {
        SectionBufferBuilderPack pack = CHECK_PACK.get();
        GAME.set(true);
        SectionCompiler.Results game;
        long start = System.nanoTime();
        try {
            game = compiler.compile(sectionPos, region, sorting, pack);
        } finally {
            GAME.set(false);
        }
        gameNanos.add(System.nanoTime() - start);
        try {
            List<String> problems = new ArrayList<>();
            for (ChunkSectionLayer layer : LAYERS) {
                compareLayer(layer, ours.renderedLayers.get(layer), game.renderedLayers.get(layer), problems, region, sectionPos);
            }
            for (Direction a : DIRECTIONS) {
                for (Direction b : DIRECTIONS) {
                    if (ours.visibilitySet.visibilityBetween(a, b) != game.visibilitySet.visibilityBetween(a, b)) {
                        problems.add("visibility " + a + "-" + b);
                    }
                }
            }
            if (!ours.blockEntities.equals(game.blockEntities)) {
                problems.add("block entities " + ours.blockEntities.size() + " vs " + game.blockEntities.size());
            }
            checked.increment();
            if (!problems.isEmpty()) {
                long n = mismatched.sum();
                mismatched.increment();
                if (n < 40) {
                    LOGGER.warn("[mesher] section {} differs from the game: {}", sectionPos, String.join("; ", problems));
                }
            }
        } finally {
            game.release();
        }
    }

    private static void compareLayer(ChunkSectionLayer layer, @Nullable MeshData ours, @Nullable MeshData game, List<String> problems,
                                     RenderSectionRegion region, SectionPos sectionPos) {
        Map<ByteBuffer, Integer> quads = new HashMap<>();
        int oursCount = add(quads, ours, 1);
        int gameCount = add(quads, game, -1);
        if (oursCount == gameCount && quads.values().stream().allMatch(v -> v == 0)) {
            return;
        }
        StringBuilder s = new StringBuilder(layer.label()).append(": ").append(oursCount).append(" quads vs the game's ").append(gameCount);
        // The same quad (positions and UVs) with other colors or light: show both side by side.
        Map<String, ByteBuffer> onlyGame = new HashMap<>();
        List<ByteBuffer> onlyOurs = new ArrayList<>();
        int wrong = 0;
        for (Map.Entry<ByteBuffer, Integer> e : quads.entrySet()) {
            if (e.getValue() < 0) {
                onlyGame.put(geometry(e.getKey()), e.getKey());
                wrong++;
            } else if (e.getValue() > 0) {
                onlyOurs.add(e.getKey());
            }
        }
        s.append(", ").append(wrong).append(" differ");
        int shown = 0;
        for (ByteBuffer q : onlyOurs) {
            if (shown >= 3) {
                break;
            }
            ByteBuffer g = onlyGame.get(geometry(q));
            s.append("\n    ours: ").append(describe(q)).append("\n    game: ").append(g == null ? "(no such quad)" : describe(g));
            if (shown == 0 && explained.getAndIncrement() < 6) {
                s.append("\n    why: ").append(explain(region, sectionPos, q));
            }
            shown++;
        }
        problems.add(s.toString());
    }

    private static int add(Map<ByteBuffer, Integer> quads, @Nullable MeshData mesh, int sign) {
        if (mesh == null) {
            return 0;
        }
        ByteBuffer v = mesh.vertexBuffer();
        int quadsCount = mesh.drawState().vertexCount() / 4;
        int stride = mesh.drawState().format().getVertexSize() * 4;
        for (int i = 0; i < quadsCount; i++) {
            byte[] q = new byte[stride];
            v.get(v.position() + i * stride, q);
            quads.merge(ByteBuffer.wrap(q), sign, Integer::sum);
        }
        return quadsCount;
    }

    private static final java.util.concurrent.atomic.AtomicInteger explained = new java.util.concurrent.atomic.AtomicInteger();
    private static final Direction[][] CORNERS = {
            {Direction.WEST, Direction.EAST, Direction.NORTH, Direction.SOUTH}, {Direction.EAST, Direction.WEST, Direction.NORTH, Direction.SOUTH},
            {Direction.UP, Direction.DOWN, Direction.EAST, Direction.WEST}, {Direction.WEST, Direction.EAST, Direction.DOWN, Direction.UP},
            {Direction.UP, Direction.DOWN, Direction.NORTH, Direction.SOUTH}, {Direction.DOWN, Direction.UP, Direction.NORTH, Direction.SOUTH}};

    /** Dev: finds the block and model quad behind a quad and asks the game's own BlockModelLighter about it. */
    private static String explain(RenderSectionRegion region, SectionPos sectionPos, ByteBuffer quad) {
        ByteBuffer q = quad.duplicate().order(java.nio.ByteOrder.LITTLE_ENDIAN);
        int stride = q.capacity() / 4;
        float[][] v = new float[4][3];
        for (int i = 0; i < 4; i++) {
            for (int c = 0; c < 3; c++) {
                v[i][c] = q.getFloat(i * stride + c * 4);
            }
        }
        BlockStateModelSet models = exportedModels;
        net.minecraft.util.RandomSource random = net.minecraft.util.RandomSource.createThreadLocalInstance(0L);
        List<net.minecraft.client.renderer.block.dispatch.BlockStateModelPart> parts = new ArrayList<>();
        BlockPos origin = sectionPos.origin();
        for (int bx = (int) Math.floor(v[0][0]) - 1; bx <= (int) Math.floor(v[0][0]) + 1; bx++) {
            for (int by = (int) Math.floor(v[0][1]) - 1; by <= (int) Math.floor(v[0][1]) + 1; by++) {
                for (int bz = (int) Math.floor(v[0][2]) - 1; bz <= (int) Math.floor(v[0][2]) + 1; bz++) {
                    BlockPos pos = origin.offset(bx, by, bz);
                    BlockState st = region.getBlockState(pos);
                    random.setSeed(st.getSeed(pos));
                    parts.clear();
                    models.get(st).collectParts(random, parts);
                    for (var part : parts) {
                        for (Direction d : DIRECTIONS) {
                            for (net.minecraft.client.resources.model.geometry.BakedQuad bq : part.getQuads(d)) {
                                boolean same = true;
                                for (int i = 0; i < 4 && same; i++) {
                                    same = bq.position(i).x() + bx == v[i][0] && bq.position(i).y() + by == v[i][1] && bq.position(i).z() + bz == v[i][2];
                                }
                                if (!same) {
                                    continue;
                                }
                                com.mojang.blaze3d.vertex.QuadInstance inst = new com.mojang.blaze3d.vertex.QuadInstance();
                                new net.minecraft.client.renderer.block.BlockModelLighter().prepareQuadAmbientOcclusion(region, st, pos, bq, inst);
                                StringBuilder s = new StringBuilder();
                                s.append(name(st)).append(" at ").append(bx).append(',').append(by).append(',').append(bz).append(' ').append(d)
                                        .append(" ao ").append(part.useAmbientOcclusion())
                                        .append(" lighter colors ").append(String.format("%08x %08x %08x %08x", inst.getColor(0), inst.getColor(1), inst.getColor(2), inst.getColor(3)));
                                BlockPos bp = pos.relative(d);
                                Direction[] c = CORNERS[d.ordinal()];
                                for (int k = 0; k < 4; k++) {
                                    BlockPos cp = bp.relative(c[k]);
                                    BlockState cs = region.getBlockState(cp);
                                    BlockState fs = region.getBlockState(cp.relative(d));
                                    s.append("\n      corner ").append(c[k]).append(' ').append(name(cs)).append(" shade ").append(cs.getShadeBrightness(region, cp))
                                            .append(" id ").append(((StateId) cs).cursed$id()).append(" | +d ").append(name(fs)).append(" permeable ").append(fs.isLightPermeable());
                                }
                                for (int[] pair : new int[][]{{0, 2}, {0, 3}, {1, 2}, {1, 3}}) {
                                    BlockPos dp = bp.relative(c[pair[0]]).relative(c[pair[1]]);
                                    BlockState ds = region.getBlockState(dp);
                                    s.append("\n      diagonal ").append(c[pair[0]]).append('+').append(c[pair[1]]).append(' ').append(name(ds)).append(" shade ").append(ds.getShadeBrightness(region, dp));
                                }
                                s.append("\n      front ").append(name(region.getBlockState(bp))).append(" shade ").append(region.getBlockState(bp).getShadeBrightness(region, bp));
                                return s.toString();
                            }
                        }
                    }
                }
            }
        }
        return "(no block found)";
    }

    private static String name(BlockState s) {
        return net.minecraft.core.registries.BuiltInRegistries.BLOCK.getKey(s.getBlock()).getPath();
    }

    /** A quad's positions and UVs, without color and light. */
    private static String geometry(ByteBuffer quad) {
        ByteBuffer q = quad.duplicate().order(java.nio.ByteOrder.LITTLE_ENDIAN);
        int stride = q.capacity() / 4;
        StringBuilder s = new StringBuilder();
        for (int v = 0; v < 4; v++) {
            int b = v * stride;
            s.append(q.getInt(b)).append(',').append(q.getInt(b + 4)).append(',').append(q.getInt(b + 8)).append(',')
                    .append(q.getInt(b + 16)).append(',').append(q.getInt(b + 20)).append(';');
        }
        return s.toString();
    }

    private static String describe(ByteBuffer quad) {
        ByteBuffer q = quad.duplicate().order(java.nio.ByteOrder.LITTLE_ENDIAN);
        StringBuilder s = new StringBuilder();
        int stride = q.capacity() / 4;
        for (int v = 0; v < 4; v++) {
            int b = v * stride;
            s.append(String.format("[%.4f %.4f %.4f #%08x uv %.5f %.5f l %08x] ",
                    q.getFloat(b), q.getFloat(b + 4), q.getFloat(b + 8), q.getInt(b + 12), q.getFloat(b + 16), q.getFloat(b + 20), q.getInt(b + 24)));
        }
        return s.toString();
    }
}
