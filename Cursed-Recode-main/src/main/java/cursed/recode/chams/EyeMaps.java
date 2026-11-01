package cursed.recode.chams;

import com.mojang.blaze3d.platform.NativeImage;
import com.mojang.blaze3d.vertex.PoseStack;
import java.io.InputStream;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.Set;
import net.minecraft.client.Minecraft;
import net.minecraft.client.model.Model;
import net.minecraft.client.model.geom.ModelPart;
import net.minecraft.client.renderer.texture.AbstractTexture;
import net.minecraft.client.renderer.texture.DynamicTexture;
import net.minecraft.resources.Identifier;
import net.minecraft.server.packs.resources.Resource;
import org.jspecify.annotations.Nullable;
import org.slf4j.Logger;

/**
 * Where a creature's eyes are, for Chams' shadow (its look "Shadow": the body black smoke, its own eyes glowing): found
 * in its own texture on its model's head, so they are where that creature has them — a creeper's high, a villager's
 * under its brow, a horse's and a goat's on the sides of the head.
 *
 * <p>Eyes that are parts of their own (a slime's, a guardian's, a frog's): their fronts. Else the head: the parts named
 * for it ("head", a wolf's "real_head", a goat's "nose" on its head), each of their cubes, the largest of a part first
 * (a cow's face before its muzzle). The eyes on a face are its texels that stand out from the face's own colour in pairs
 * mirrored across its middle, in the row that most looks like eyes (the darkest, the whitest, the most coloured of them;
 * never a row that runs across the middle: a brow, a mouth), and the row next to it when it is as much so. Those on the
 * two sides of a cube, mirrored from one to the other, when there are none on the front or they are much more like eyes;
 * first for the creatures whose fronts are muzzles (a horse's nostrils are not eyes). A creature without a head: its
 * body's front. The game's own glowing layers (a spider's, an enderman's, a breeze's, a copper golem's) are its eyes;
 * those of a few are known by hand (KNOWN).
 *
 * <p>The texture is copied with its eye texels marked in their alpha (253; a texel that was 253 already becomes 254),
 * each carrying its eye's slit in its colour (slits); registered beside it, the shadow's render types bind the copy
 * (Chams.make) and its shader draws the slits (chams.glsl ch_slit).
 */
public final class EyeMaps {
    /** The alpha that marks an eye (chams.glsl CH_EYE). */
    public static final int MARK = 253;
    /** The game's own eye layers, by the texture they are of. */
    private static final Map<String, String> LAYERS = Map.ofEntries(
            Map.entry("textures/entity/spider/spider.png", "textures/entity/spider/spider_eyes.png"),
            Map.entry("textures/entity/spider/cave_spider.png", "textures/entity/spider/spider_eyes.png"),
            Map.entry("textures/entity/enderman/enderman.png", "textures/entity/enderman/enderman_eyes.png"),
            Map.entry("textures/entity/phantom/phantom.png", "textures/entity/phantom/phantom_eyes.png"),
            Map.entry("textures/entity/breeze/breeze.png", "textures/entity/breeze/breeze_eyes.png"),
            Map.entry("textures/entity/creaking/creaking.png", "textures/entity/creaking/creaking_eyes.png"),
            Map.entry("textures/entity/enderdragon/dragon.png", "textures/entity/enderdragon/dragon_eyes.png"),
            Map.entry("textures/entity/copper_golem/copper_golem.png", "textures/entity/copper_golem/copper_golem_eyes.png"),
            Map.entry("textures/entity/copper_golem/copper_golem_exposed.png", "textures/entity/copper_golem/copper_golem_eyes_exposed.png"),
            Map.entry("textures/entity/copper_golem/copper_golem_weathered.png", "textures/entity/copper_golem/copper_golem_eyes_weathered.png"),
            Map.entry("textures/entity/copper_golem/copper_golem_oxidized.png", "textures/entity/copper_golem/copper_golem_eyes_oxidized.png"));
    /**
     * Eyes known by hand (the game's texture's width, then its texels: x, y each), where what is found is not them: a
     * zombified piglin's one eye and its socket, a creeper's among its speckles, a snow golem's coals out of line, a
     * wandering trader's under its hood, a baby happy ghast's shut, a magma cube's on its slices, a sniffer's black on
     * its dark red, a camel's high on its head (its snout's sides are its nostrils), a bee's beside its stripes, a
     * skeleton foal's sockets, a pufferfish's of its three sizes (one texture, the first model seen would mark only its
     * own), a sulfur cube's on its outer cube. None (the width alone): the warden is blind.
     */
    private static final Map<String, int[]> KNOWN = Map.ofEntries(
            Map.entry("textures/entity/piglin/zombified_piglin.png", new int[] {64, 10, 11, 15, 11}),
            Map.entry("textures/entity/piglin/zombified_piglin_baby.png", new int[] {64, 9, 9, 10, 9, 9, 10, 10, 10, 13, 9}),
            Map.entry("textures/entity/creeper/creeper.png", new int[] {64, 9, 10, 10, 10, 13, 10, 14, 10, 9, 11, 10, 11, 13, 11, 14, 11}),
            Map.entry("textures/entity/snow_golem/snow_golem.png", new int[] {64, 10, 9, 14, 10}),
            Map.entry("textures/entity/wandering_trader/wandering_trader.png", new int[] {64, 9, 14, 10, 14, 13, 14, 14, 14}),
            Map.entry("textures/entity/ghast/happy_ghast_baby.png", new int[] {64, 18, 24, 19, 24, 20, 24, 27, 24, 28, 24, 29, 24}),
            Map.entry("textures/entity/slime/magmacube.png", new int[] {64, 9, 26, 10, 26, 13, 26, 14, 26, 9, 35, 10, 35, 13, 35, 14, 35}),
            Map.entry("textures/entity/sniffer/sniffer.png", new int[] {192, 13, 31, 14, 31, 15, 31, 16, 31, 34, 31, 35, 31, 36, 31, 37, 31}),
            Map.entry("textures/entity/camel/camel.png", new int[] {128, 26, 8, 27, 8, 28, 8, 34, 8, 35, 8, 36, 8}),
            Map.entry("textures/entity/camel/camel_husk.png", new int[] {128, 26, 8, 27, 8, 28, 8, 34, 8, 35, 8, 36, 8}),
            Map.entry("textures/entity/camel/camel_baby.png", new int[] {64, 3, 6, 4, 6, 10, 6, 11, 6}),
            Map.entry("textures/entity/bee/bee.png", new int[] {64, 10, 13, 11, 13, 15, 13, 16, 13, 11, 14, 15, 14}),
            Map.entry("textures/entity/horse/horse_skeleton_baby.png", new int[] {64, 2, 10, 3, 10, 3, 11, 20, 10, 21, 10, 20, 11}),
            Map.entry("textures/entity/fish/pufferfish.png", new int[] {32, 9, 9, 10, 9, 13, 9, 14, 9, 17, 28, 21, 28, 3, 29, 5, 29}),
            Map.entry("textures/entity/sulfur_cube/sulfur_cube_outer.png",
                    new int[] {128, 21, 28, 22, 28, 20, 29, 21, 29, 22, 29, 20, 30, 21, 30, 22, 30, 32, 28, 33, 28, 31, 29, 32, 29, 33, 29, 31, 30, 32, 30, 33, 30}),
            Map.entry("textures/entity/sulfur_cube/sulfur_cube_outer_small.png", new int[] {64, 12, 16, 11, 17, 12, 17, 18, 16, 17, 17, 18, 17}),
            Map.entry("textures/entity/warden/warden.png", new int[] {128}));
    /** Creatures whose eyes are on the sides of the head, their front a muzzle (nostrils are not eyes). */
    private static final List<String> SIDE_EYED = List.of("textures/entity/horse/", "textures/entity/camel/", "textures/entity/sniffer/");
    /** Each texture's copy with its eyes marked (itself when none were found or it could not be read). */
    private static final Map<Identifier, Identifier> MARKED = new HashMap<>();
    private static int made;
    private static final Logger LOGGER = com.mojang.logging.LogUtils.getLogger();
    /** Dev (CURSED_DEV_EYELOG): each texture's head faces and the eyes found, in the log. */
    private static final boolean LOG = cursed.recode.dev.Dev.on("CURSED_DEV_EYELOG");

    private EyeMaps() {
    }

    /** The texture a shadow's body binds for `texture`: its copy with the eyes marked, once made. */
    public static Identifier marked(Identifier texture) {
        return MARKED.getOrDefault(texture, texture);
    }

    /** Whether `texture` has been looked at already. */
    public static boolean known(Identifier texture) {
        return MARKED.containsKey(texture);
    }

    /** The resources reloaded (EntityRenderDispatcherMixin): the copies let go, made again from the new textures. */
    public static void reset() {
        MARKED.forEach((texture, copy) -> {
            if (!copy.equals(texture)) {
                Minecraft.getInstance().getTextureManager().release(copy);
            }
        });
        MARKED.clear();
        Chams.forgetShadow();
    }

    /**
     * Before a body's model is submitted in the shadow (ChamsSubmitMixin): its texture's eyes found and its copy made,
     * once a texture. On the render thread (the copy goes to the GPU).
     */
    public static void prepare(Identifier texture, Model<?> model) {
        if (MARKED.containsKey(texture)) {
            return;
        }
        MARKED.put(texture, texture);
        try {
            NativeImage image = read(texture);
            if (image == null) {
                return;
            }
            try {
                int w = image.getWidth();
                int h = image.getHeight();
                boolean[][] eyes = layerEyes(texture, w, h);
                if (eyes == null) {
                    eyes = known(texture, w, h);
                }
                if (eyes == null) {
                    eyes = find(image, model, SIDE_EYED.stream().anyMatch(texture.getPath()::startsWith));
                }
                if (LOG) {
                    log(texture, model, w, h, eyes);
                }
                if (eyes == null) {
                    return;
                }
                NativeImage copy = new NativeImage(w, h, false);
                copy.copyFrom(image);
                int[][] slit = slits(eyes, w, h);
                for (int y = 0; y < h; y++) {
                    for (int x = 0; x < w; x++) {
                        int c = copy.getPixel(x, y);
                        if (eyes[x][y]) {
                            copy.setPixel(x, y, (MARK << 24) | slit[x][y]);
                        } else if (c >>> 24 == MARK) {
                            copy.setPixel(x, y, ((MARK + 1) << 24) | (c & 0xffffff));
                        }
                    }
                }
                Identifier id = Identifier.fromNamespaceAndPath("cursed-recode", "eyes/" + (made++) + "/" + texture.getNamespace() + "/" + texture.getPath());
                Minecraft.getInstance().getTextureManager().register(id, new DynamicTexture(id::toString, copy));
                MARKED.put(texture, id);
            } finally {
                image.close();
            }
        } catch (Throwable t) {
            MARKED.put(texture, texture);
        }
    }

    /**
     * Each eye texel's slit as its colour (the shadow's shader reads it, chams.glsl ch_slit): an eye is its texels that
     * touch (corners too), its slit a line across the middle of them as wide as they are (a texel and a half at least: a
     * villager's one white), red and green its ends across and blue its middle down, each in half texels from the
     * texel's own corner, 128 at it.
     */
    private static int[][] slits(boolean[][] eyes, int w, int h) {
        int[][] out = new int[w][h];
        boolean[][] seen = new boolean[w][h];
        int[] stack = new int[w * h];
        List<int[]> eye = new ArrayList<>();
        for (int y0 = 0; y0 < h; y0++) {
            for (int x0 = 0; x0 < w; x0++) {
                if (!eyes[x0][y0] || seen[x0][y0]) {
                    continue;
                }
                eye.clear();
                int n = 0;
                stack[n++] = y0 * w + x0;
                seen[x0][y0] = true;
                int minX = x0, maxX = x0, minY = y0, maxY = y0;
                while (n > 0) {
                    int i = stack[--n];
                    int x = i % w;
                    int y = i / w;
                    eye.add(new int[] {x, y});
                    minX = Math.min(minX, x);
                    maxX = Math.max(maxX, x);
                    minY = Math.min(minY, y);
                    maxY = Math.max(maxY, y);
                    for (int dy = -1; dy <= 1; dy++) {
                        for (int dx = -1; dx <= 1; dx++) {
                            int nx = x + dx;
                            int ny = y + dy;
                            if (nx >= 0 && ny >= 0 && nx < w && ny < h && eyes[nx][ny] && !seen[nx][ny]) {
                                seen[nx][ny] = true;
                                stack[n++] = ny * w + nx;
                            }
                        }
                    }
                }
                double mid = (minX + maxX + 1) / 2.0;
                double half = Math.max((maxX + 1 - minX) / 2.0, 0.75);
                double down = (minY + maxY + 1) / 2.0;
                for (int[] e : eye) {
                    int r = half(mid - half - e[0]);
                    int g = half(mid + half - e[0]);
                    int b = half(down - e[1]);
                    out[e[0]][e[1]] = r << 16 | g << 8 | b;
                }
            }
        }
        return out;
    }

    /** A length in texels as half texels from 128, within a byte. */
    private static int half(double texels) {
        return Math.clamp(Math.round(128 + texels * 2.0), 0, 255);
    }

    /** A copy of the texture's pixels: a downloaded skin's from its texture, the game's from its resources. */
    private static @Nullable NativeImage read(Identifier texture) throws Exception {
        Minecraft mc = Minecraft.getInstance();
        AbstractTexture t = mc.getTextureManager().getTexture(texture);
        if (t instanceof DynamicTexture d && d.getPixels() != null) {
            NativeImage copy = new NativeImage(d.getPixels().getWidth(), d.getPixels().getHeight(), false);
            copy.copyFrom(d.getPixels());
            return copy;
        }
        Optional<Resource> r = mc.getResourceManager().getResource(texture);
        if (r.isEmpty()) {
            return null;
        }
        try (InputStream in = r.get().open()) {
            return NativeImage.read(in);
        }
    }

    /** The eyes of a creature whose game draws them glowing: that layer's texels that show. */
    private static boolean @Nullable [][] layerEyes(Identifier texture, int w, int h) throws Exception {
        String layer = LAYERS.get(texture.getPath());
        if (layer == null) {
            return null;
        }
        NativeImage eyes = read(Identifier.withDefaultNamespace(layer));
        if (eyes == null) {
            return null;
        }
        try {
            boolean[][] out = new boolean[w][h];
            boolean any = false;
            for (int y = 0; y < Math.min(h, eyes.getHeight()); y++) {
                for (int x = 0; x < Math.min(w, eyes.getWidth()); x++) {
                    int c = eyes.getPixel(x, y);
                    if ((c >>> 24) > 32 && (c & 0xffffff) != 0) {
                        out[x][y] = true;
                        any = true;
                    }
                }
            }
            return any ? out : null;
        } finally {
            eyes.close();
        }
    }

    /** Eyes known by hand, scaled to the texture's size (a pack's larger one). */
    private static boolean @Nullable [][] known(Identifier texture, int w, int h) {
        int[] k = KNOWN.get(texture.getPath());
        if (k != null && k.length == 1) {
            return new boolean[w][h];
        }
        if (k == null || w < k[0]) {
            return null;
        }
        int s = w / k[0];
        boolean[][] out = new boolean[w][h];
        for (int i = 1; i + 1 < k.length; i += 2) {
            for (int dy = 0; dy < s; dy++) {
                for (int dx = 0; dx < s; dx++) {
                    out[k[i] * s + dx][k[i + 1] * s + dy] = true;
                }
            }
        }
        return out;
    }

    /** A cube's face in the texture (texels: x, y, width, height). */
    private record Face(int x, int y, int w, int h) {
    }

    /** The faces of a cube by the way they look out: north (its front), west, east; whether it is the head's own (not
     *  a nose on it, not the body: those are looked at only when the head has none). */
    private record Faces(@Nullable Face north, @Nullable Face west, @Nullable Face east, boolean own) {
    }

    private static @Nullable Face face(ModelPart.Polygon p, int w, int h) {
        float u0 = 1.0F;
        float v0 = 1.0F;
        float u1 = 0.0F;
        float v1 = 0.0F;
        for (ModelPart.Vertex v : p.vertices()) {
            u0 = Math.min(u0, v.u());
            v0 = Math.min(v0, v.v());
            u1 = Math.max(u1, v.u());
            v1 = Math.max(v1, v.v());
        }
        int x0 = Math.round(u0 * w);
        int y0 = Math.round(v0 * h);
        int fw = Math.round(u1 * w) - x0;
        int fh = Math.round(v1 * h) - y0;
        return fw >= 2 && fh >= 2 && x0 >= 0 && y0 >= 0 && x0 + fw <= w && y0 + fh <= h ? new Face(x0, y0, fw, fh) : null;
    }

    /** The cubes eyes may be on: those of the parts named for the head (else the body's), in the order drawn. */
    private static List<Faces> heads(Model<?> model, int w, int h) {
        List<Faces> heads = new ArrayList<>();
        List<Faces> bodies = new ArrayList<>();
        List<String> paths = new ArrayList<>();
        Map<String, Integer> largest = new HashMap<>();
        model.root().visit(new PoseStack(), (pose, path, index, cube) -> {
            String last = path.substring(path.lastIndexOf('/') + 1);
            boolean head = last.contains("head") || last.equals("nose") && path.contains("head");
            boolean body = last.equals("body") || last.equals("bone");
            if (!head && !body || last.contains("hat") || last.contains("ear") || last.contains("horn")) {
                return;
            }
            Face n = null;
            Face west = null;
            Face east = null;
            for (ModelPart.Polygon p : cube.polygons) {
                if (p.normal().z() < -0.5F) {
                    n = face(p, w, h);
                } else if (p.normal().x() < -0.5F) {
                    west = face(p, w, h);
                } else if (p.normal().x() > 0.5F) {
                    east = face(p, w, h);
                }
            }
            boolean own = head && !last.equals("nose") && n != null && n.w() * n.h() >= 9;
            if (head) {
                paths.add(path);
                largest.merge(path, n == null ? 0 : n.w() * n.h(), Math::max);
            }
            (head ? heads : bodies).add(new Faces(n, west, east, own));
        });
        // a head's own cube is the first of the largest of its part, large enough to have a face (a goat's head is a
        // stub, its face on its nose; a cow's muzzle is on the head beside its face; a parched's hood is over it)
        Set<String> given = new HashSet<>();
        for (int i = 0; i < heads.size(); i++) {
            Faces f = heads.get(i);
            if (f.own() && (f.north().w() * f.north().h() < largest.get(paths.get(i)) || !given.add(paths.get(i)))) {
                heads.set(i, new Faces(f.north(), f.west(), f.east(), false));
            }
        }
        return heads.isEmpty() ? bodies : heads;
    }

    /**
     * The eye texels on the head (the whole texture's grid), or null when there are none to be found; `sideEyed`: those
     * on the sides of any of its cubes before any front.
     */
    static boolean @Nullable [][] find(NativeImage image, Model<?> model, boolean sideEyed) {
        int w = image.getWidth();
        int h = image.getHeight();
        boolean[][] out = new boolean[w][h];
        // eyes that are parts of their own (a slime's, a guardian's): their fronts whole
        boolean[] parts = {false};
        model.root().visit(new PoseStack(), (pose, path, index, cube) -> {
            String last = path.substring(path.lastIndexOf('/') + 1);
            if (!last.contains("eye") || last.contains("brow") || last.contains("lid")) {
                return;
            }
            for (ModelPart.Polygon p : cube.polygons) {
                Face f = p.normal().z() < -0.5F ? face(p, w, h) : null;
                if (f == null) {
                    continue;
                }
                for (int y = 0; y < f.h(); y++) {
                    for (int x = 0; x < f.w(); x++) {
                        if ((image.getPixel(f.x() + x, f.y() + y) >>> 24) > 0) {
                            out[f.x() + x][f.y() + y] = true;
                            parts[0] = true;
                        }
                    }
                }
            }
        });
        if (parts[0]) {
            return out;
        }
        List<Faces> heads = heads(model, w, h);
        if (sideEyed) {
            Faces best = null;
            float bestScore = 0.3F;
            for (Faces f : heads) {
                float t = pairedSides(f) ? sides(image, f.west(), f.east(), null) : 0.0F;
                if (t > bestScore) {
                    bestScore = t;
                    best = f;
                }
            }
            if (best != null) {
                sides(image, best.west(), best.east(), out);
                return out;
            }
        }
        // the head's own cubes, then the rest: the front most like eyes, or the sides when theirs are more so
        for (boolean own : new boolean[] {true, false}) {
            Faces bestFront = null;
            float frontScore = 0.3F;
            Faces bestSides = null;
            float sideScore = 0.3F;
            for (Faces f : heads) {
                if (f.own() != own) {
                    continue;
                }
                float s = f.north() == null ? 0.0F : front(image, f.north(), null);
                if (s > frontScore) {
                    frontScore = s;
                    bestFront = f;
                }
                float t = pairedSides(f) ? sides(image, f.west(), f.east(), null) : 0.0F;
                if (t > sideScore) {
                    sideScore = t;
                    bestSides = f;
                }
            }
            if (bestSides != null && (bestFront == null || sideScore > frontScore * 1.25F)) {
                sides(image, bestSides.west(), bestSides.east(), out);
                return out;
            }
            if (bestFront != null) {
                front(image, bestFront.north(), out);
                return out;
            }
        }
        // a player's skin with no eyes to be found: where the game's own skins have them
        if (!heads.isEmpty() && heads.get(0).north() != null && heads.get(0).north().w() == 8 * (w / 64) && heads.get(0).north().h() == 8 * (w / 64) && w >= 64) {
            Face f = heads.get(0).north();
            int s = w / 64;
            for (int[] e : new int[][] {{1, 4}, {2, 4}, {5, 4}, {6, 4}}) {
                for (int dy = 0; dy < s; dy++) {
                    for (int dx = 0; dx < s; dx++) {
                        out[f.x() + e[0] * s + dx][f.y() + e[1] * s + dy] = true;
                    }
                }
            }
            return out;
        }
        return null;
    }

    /** Whether a cube's sides may hold a pair of eyes: it has a front (a goat's beard is a plane: not), they match. */
    private static boolean pairedSides(Faces f) {
        return f.north() != null && f.west() != null && f.east() != null && f.west().w() == f.east().w() && f.west().h() == f.east().h();
    }

    /** How a face's texel stands out of its own colour as an eye would (0 not at all): dark, white or coloured. */
    private static float[][] standing(NativeImage image, Face f) {
        Map<Integer, Integer> counts = new HashMap<>();
        for (int y = 0; y < f.h(); y++) {
            for (int x = 0; x < f.w(); x++) {
                int c = image.getPixel(f.x() + x, f.y() + y);
                if ((c >>> 24) > 0) {
                    counts.merge(key(c), 1, Integer::sum);
                }
            }
        }
        float[][] out = new float[f.w()][f.h()];
        if (counts.isEmpty()) {
            return out;
        }
        int dominant = counts.entrySet().stream().max(Map.Entry.comparingByValue()).get().getKey();
        long[] sum = new long[4];
        for (int y = 0; y < f.h(); y++) {
            for (int x = 0; x < f.w(); x++) {
                int c = image.getPixel(f.x() + x, f.y() + y);
                if ((c >>> 24) > 0 && key(c) == dominant) {
                    sum[0] += (c >> 16) & 255;
                    sum[1] += (c >> 8) & 255;
                    sum[2] += c & 255;
                    sum[3]++;
                }
            }
        }
        float[] dom = {sum[0] / (float) sum[3], sum[1] / (float) sum[3], sum[2] / (float) sum[3]};
        float domLum = (0.2126F * dom[0] + 0.7152F * dom[1] + 0.0722F * dom[2]) / 255.0F;
        for (int y = 0; y < f.h(); y++) {
            for (int x = 0; x < f.w(); x++) {
                int c = image.getPixel(f.x() + x, f.y() + y);
                if ((c >>> 24) == 0) {
                    continue;
                }
                float r = (c >> 16) & 255;
                float g = (c >> 8) & 255;
                float b = c & 255;
                float diff = Math.abs(r - dom[0]) + Math.abs(g - dom[1]) + Math.abs(b - dom[2]);
                // against the texels round it on the face: a white pupil in a dark patch stands out of it
                float local = 0.0F;
                int around = 0;
                float[] mean = new float[3];
                for (int dy = -1; dy <= 1; dy++) {
                    for (int dx = -1; dx <= 1; dx++) {
                        int nx = x + dx;
                        int ny = y + dy;
                        if ((dx != 0 || dy != 0) && nx >= 0 && ny >= 0 && nx < f.w() && ny < f.h()) {
                            int n = image.getPixel(f.x() + nx, f.y() + ny);
                            mean[0] += (n >> 16) & 255;
                            mean[1] += (n >> 8) & 255;
                            mean[2] += n & 255;
                            around++;
                        }
                    }
                }
                if (around > 0) {
                    local = Math.abs(r - mean[0] / around) + Math.abs(g - mean[1] / around) + Math.abs(b - mean[2] / around);
                }
                float lum = (0.2126F * r + 0.7152F * g + 0.0722F * b) / 255.0F;
                // or out of the texels round it: a white pupil in a dark patch, a black eye in a pale one
                if (diff < 90.0F && !(local >= 240.0F && lum > 0.6F) && !(local >= 200.0F && lum < 0.2F)) {
                    continue;
                }
                float max = Math.max(r, Math.max(g, b));
                float sat = max <= 0.0F ? 0.0F : (max - Math.min(r, Math.min(g, b))) / max;
                // white is an eye's when an iris or a pupil is beside it (darker by much), or it is a pupil in the dark
                boolean white = lum > 0.72F && (local >= 240.0F || darkBeside(image, f, x, y, lum - 0.25F));
                out[x][y] = Math.max(diff, local * 1.2F) / 765.0F + (white ? 0.5F : 0.0F) + (lum < 0.13F ? 0.35F : 0.0F) + (sat > 0.45F ? 0.25F : 0.0F);
            }
        }
        return out;
    }

    /**
     * Whether a texel beside this one on the face (left or right) is darker than `below` (its iris, its pupil); not one
     * in the middle (a nose between the eyes).
     */
    private static boolean darkBeside(NativeImage image, Face f, int x, int y, float below) {
        for (int dx : new int[] {-1, 1}) {
            int nx = x + dx;
            if (nx < 0 || nx >= f.w() || nx == (f.w() - 1) / 2 || nx == f.w() / 2) {
                continue;
            }
            int n = image.getPixel(f.x() + nx, f.y() + y);
            float l = (0.2126F * ((n >> 16) & 255) + 0.7152F * ((n >> 8) & 255) + 0.0722F * (n & 255)) / 255.0F;
            if ((n >>> 24) > 0 && l < below) {
                return true;
            }
        }
        return false;
    }

    /**
     * The eyes on a front face, in pairs mirrored across its middle: how much they are like eyes (0 none), marked into
     * `out` when it is given.
     */
    private static float front(NativeImage image, Face f, boolean @Nullable [][] out) {
        float[][] s = standing(image, f);
        int w = f.w();
        int top = (int) Math.floor(f.h() * 0.12);
        int bottom = (int) Math.ceil(f.h() * 0.7);
        float[] score = new float[f.h()];
        for (int y = top; y < bottom; y++) {
            score[y] = across(image, f, s, y) ? 0.0F : rowScore(s, w, y);
        }
        int best = -1;
        for (int y = top; y < bottom; y++) {
            if (score[y] > 0.3F && (best < 0 || score[y] > score[best] + 1e-4F)) {
                best = y;
            }
        }
        if (best < 0) {
            return 0.0F;
        }
        if (out != null) {
            markRow(s, w, best, f, out);
            // two rows tall when the next is as much so
            for (int y : new int[] {best - 1, best + 1}) {
                if (y >= top && y < bottom && score[y] >= score[best] * 0.7F) {
                    markRow(s, w, y, f, out);
                    break;
                }
            }
        }
        return score[best];
    }

    /**
     * Whether a row runs across the face's middle as one line (a brow, a mouth): its middle stands out and is of the
     * colour of what stands beside it (a wolf's brown muzzle between its black eyes is not).
     */
    private static boolean across(NativeImage image, Face f, float[][] s, int y) {
        int w = f.w();
        int m0 = (w - 1) / 2;
        int m1 = w / 2;
        for (int m : new int[] {m0, m1}) {
            if (s[m][y] <= 0.0F) {
                continue;
            }
            int c = image.getPixel(f.x() + m, f.y() + y);
            for (int x = 0; x < w; x++) {
                if (x == m0 || x == m1 || s[x][y] <= 0.0F || s[w - 1 - x][y] <= 0.0F) {
                    continue;
                }
                int o = image.getPixel(f.x() + x, f.y() + y);
                int d = Math.abs(((c >> 16) & 255) - ((o >> 16) & 255)) + Math.abs(((c >> 8) & 255) - ((o >> 8) & 255)) + Math.abs((c & 255) - (o & 255));
                if (d < 70) {
                    return true;
                }
            }
        }
        return false;
    }

    /** Whether `x` and its mirror `m` are a pair of eyes' texels: both stand out, neither in the middle (between the
     *  eyes: a nose, a muzzle). */
    private static boolean paired(float[][] s, int w, int x, int y) {
        int m = w - 1 - x;
        int m0 = (w - 1) / 2;
        int m1 = w / 2;
        return m != x && x != m0 && x != m1 && s[x][y] > 0.0F && s[m][y] > 0.0F;
    }

    /** A row's eyes: the mean of its most eye-like pairs (see markRow), none when it has none. */
    private static float rowScore(float[][] s, int w, int y) {
        float most = 0.0F;
        for (int x = 0; x < w; x++) {
            if (paired(s, w, x, y)) {
                most = Math.max(most, s[x][y]);
            }
        }
        float sum = 0.0F;
        int n = 0;
        for (int x = 0; x < w; x++) {
            if (paired(s, w, x, y) && s[x][y] >= most * 0.7F) {
                sum += s[x][y];
                n++;
            }
        }
        return n == 0 ? 0.0F : sum / n;
    }

    /** A row's eyes into `out`: of its pairs those most like eyes (a panda's pupil, not the dark patch round it). */
    private static void markRow(float[][] s, int w, int y, Face f, boolean[][] out) {
        float most = 0.0F;
        for (int x = 0; x < w; x++) {
            if (paired(s, w, x, y)) {
                most = Math.max(most, Math.max(s[x][y], s[w - 1 - x][y]));
            }
        }
        for (int x = 0; x < w; x++) {
            if (paired(s, w, x, y) && Math.max(s[x][y], s[w - 1 - x][y]) >= most * 0.7F) {
                out[f.x() + x][f.y() + y] = true;
            }
        }
    }

    /** The eyes on the two sides of a head, mirrored from one to the other: how much like eyes (0 none), marked into
     *  `out` when it is given. */
    private static float sides(NativeImage image, Face west, Face east, boolean @Nullable [][] out) {
        float[][] a = standing(image, west);
        float[][] b = standing(image, east);
        int w = west.w();
        int top = west.h() >= 4 ? Math.max(1, (int) Math.floor(west.h() * 0.1)) : 0;
        int bottom = (int) Math.ceil(west.h() * 0.75);
        // the west face's last column and the east's first are against the front: its colour wrapping round
        int across = w >= 4 ? w - 1 : w;
        int best = -1;
        float bestScore = 0.3F;
        float bestMost = 0.0F;
        for (int y = top; y < bottom; y++) {
            float most = 0.0F;
            for (int x = 0; x < across; x++) {
                if (pair(a, b, x, y)) {
                    most = Math.max(most, a[x][y] + b[w - 1 - x][y]);
                }
            }
            float sum = 0.0F;
            int n = 0;
            for (int x = 0; x < across; x++) {
                if (pair(a, b, x, y) && a[x][y] + b[w - 1 - x][y] >= most * 0.7F) {
                    sum += a[x][y] + b[w - 1 - x][y];
                    n++;
                }
            }
            // a few texels, not a stripe along the side
            if (n > 0 && n <= Math.max(2, w / 2) && sum / (2 * n) > bestScore) {
                bestScore = sum / (2 * n);
                best = y;
                bestMost = most;
            }
        }
        if (best < 0) {
            return 0.0F;
        }
        if (out != null) {
            boolean last = false;
            for (int x = 0; x < w; x++) {
                // the edge against the front: only where the eye runs on to it (a camel's)
                boolean eye = pair(a, b, x, best) && a[x][best] + b[w - 1 - x][best] >= bestMost * 0.7F && (x < across || last);
                if (eye) {
                    out[west.x() + x][west.y() + best] = true;
                    out[east.x() + w - 1 - x][east.y() + best] = true;
                }
                last = eye;
            }
        }
        return bestScore;
    }

    /** Whether the sides' texels `x` (west) and its mirror (east) both stand out in row `y`. */
    private static boolean pair(float[][] a, float[][] b, int x, int y) {
        return a[x][y] > 0.0F && b[a.length - 1 - x][y] > 0.0F;
    }

    /** A colour as one of 4096 (its sixteenths of each). */
    private static int key(int argb) {
        return ((argb >> 20) & 15) << 8 | ((argb >> 12) & 15) << 4 | ((argb >> 4) & 15);
    }

    /** Dev: the texture's head cubes' faces and the eye texels found, in the log. */
    private static void log(Identifier texture, Model<?> model, int w, int h, boolean @Nullable [][] eyes) {
        StringBuilder sb = new StringBuilder("[eyes] " + texture + " " + w + "x" + h);
        for (Faces f : heads(model, w, h)) {
            sb.append(" |");
            for (Face x : new Face[] {f.north(), f.west(), f.east()}) {
                sb.append(x == null ? " -" : " " + x.x() + "," + x.y() + "," + x.w() + "," + x.h());
            }
        }
        sb.append(" eyes:");
        if (eyes != null) {
            for (int y = 0; y < h; y++) {
                for (int x = 0; x < w; x++) {
                    if (eyes[x][y]) {
                        sb.append(' ').append(x).append(',').append(y);
                    }
                }
            }
        }
        LOGGER.info(sb.toString());
    }
}
