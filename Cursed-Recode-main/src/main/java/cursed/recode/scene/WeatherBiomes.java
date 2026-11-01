package cursed.recode.scene;

import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.core.BlockPos;
import net.minecraft.core.SectionPos;
import net.minecraft.util.LinearCongruentialGenerator;
import net.minecraft.util.Mth;
import net.minecraft.world.level.biome.Biome;
import net.minecraft.world.level.biome.BiomeManager;

/**
 * ClientLevel.getPrecipitationAt for the weather's columns (WeatherEffectRenderer, every frame of rain or snow, a few
 * hundred columns). The costly part of the game's biome lookup is BiomeManager's fuzzy zoom — which of the eight
 * nearby quarter-cells a block takes its biome from — and that is a pure function of the position and the world's
 * zoom seed. So the chosen cell is remembered for the columns around the camera's block (until it moves); the cell's
 * biome is still read from the chunk every time, and the rest is the game's own code: the same answers. Render thread.
 */
public final class WeatherBiomes {
    private static final long MISSING = Long.MIN_VALUE;
    /** The chosen cells of the columns around the camera's block, row by row; refilled when that block moves. */
    private static long[] cells = new long[0];
    private static BiomeManager manager;
    private static int camX;
    private static int camY = Integer.MIN_VALUE;
    private static int camZ;
    private static int radius = -1;

    private WeatherBiomes() {
    }

    /** WeatherEffectRenderer.extractRenderState's start: the camera's block and the weather's radius. */
    public static void begin(BiomeManager m, int x, int y, int z, int r) {
        if (m != manager || x != camX || y != camY || z != camZ || r != radius) {
            manager = m;
            camX = x;
            camY = y;
            camZ = z;
            radius = r;
            int n = (2 * r + 1) * (2 * r + 1);
            if (cells.length < n) {
                cells = new long[n];
            }
            java.util.Arrays.fill(cells, 0, n, MISSING);
        }
    }

    public static Biome.Precipitation precipitation(ClientLevel level, BlockPos pos) {
        if (!level.getChunkSource().hasChunk(SectionPos.blockToSectionCoord(pos.getX()), SectionPos.blockToSectionCoord(pos.getZ()))) {
            return Biome.Precipitation.NONE;
        }
        BiomeManager m = level.getBiomeManager();
        int x = pos.getX();
        int y = pos.getY();
        int z = pos.getZ();
        int dx = x - camX + radius;
        int dz = z - camZ + radius;
        int side = 2 * radius + 1;
        long cell;
        if (m == manager && y == camY && dx >= 0 && dz >= 0 && dx < side && dz < side) {
            int at = dx + dz * side;
            cell = cells[at];
            if (cell == MISSING) {
                cell = cells[at] = cell(m.biomeZoomSeed, x, y, z);
            }
        } else {
            cell = cell(m.biomeZoomSeed, x, y, z);
        }
        Biome biome = m.getNoiseBiomeAtQuart(BlockPos.getX(cell), BlockPos.getY(cell), BlockPos.getZ(cell)).value();
        return biome.getPrecipitationAt(pos, level.getSeaLevel());
    }

    /** BiomeManager.getBiome's choice of quarter-cell, as the game computes it (packed as a BlockPos). */
    static long cell(long seed, int x, int y, int z) {
        int i = x - 2;
        int j = y - 2;
        int k = z - 2;
        int l = i >> 2;
        int i1 = j >> 2;
        int j1 = k >> 2;
        double d0 = (i & 3) / 4.0;
        double d1 = (j & 3) / 4.0;
        double d2 = (k & 3) / 4.0;
        int k1 = 0;
        double d3 = Double.POSITIVE_INFINITY;
        for (int l1 = 0; l1 < 8; l1++) {
            boolean flag = (l1 & 4) == 0;
            boolean flag1 = (l1 & 2) == 0;
            boolean flag2 = (l1 & 1) == 0;
            int i2 = flag ? l : l + 1;
            int j2 = flag1 ? i1 : i1 + 1;
            int k2 = flag2 ? j1 : j1 + 1;
            double d4 = flag ? d0 : d0 - 1.0;
            double d5 = flag1 ? d1 : d1 - 1.0;
            double d6 = flag2 ? d2 : d2 - 1.0;
            double d7 = fiddledDistance(seed, i2, j2, k2, d4, d5, d6);
            if (d3 > d7) {
                k1 = l1;
                d3 = d7;
            }
        }
        int l2 = (k1 & 4) == 0 ? l : l + 1;
        int i3 = (k1 & 2) == 0 ? i1 : i1 + 1;
        int j3 = (k1 & 1) == 0 ? j1 : j1 + 1;
        return BlockPos.asLong(l2, i3, j3);
    }

    private static double fiddledDistance(long seed, int xRandom, int yRandom, int zRandom, double distanceX, double distanceY, double distanceZ) {
        long i = seed;
        i = LinearCongruentialGenerator.next(i, xRandom);
        i = LinearCongruentialGenerator.next(i, yRandom);
        i = LinearCongruentialGenerator.next(i, zRandom);
        i = LinearCongruentialGenerator.next(i, xRandom);
        i = LinearCongruentialGenerator.next(i, yRandom);
        i = LinearCongruentialGenerator.next(i, zRandom);
        double d0 = fiddle(i);
        i = LinearCongruentialGenerator.next(i, seed);
        double d1 = fiddle(i);
        i = LinearCongruentialGenerator.next(i, seed);
        double d2 = fiddle(i);
        return Mth.square(distanceZ + d2) + Mth.square(distanceY + d1) + Mth.square(distanceX + d0);
    }

    private static double fiddle(long rval) {
        double d0 = Math.floorMod(rval >> 24, 1024) / 1024.0;
        return (d0 - 0.5) * 0.9;
    }
}
