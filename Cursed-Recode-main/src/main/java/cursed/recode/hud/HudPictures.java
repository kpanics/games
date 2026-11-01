package cursed.recode.hud;

import com.mojang.blaze3d.platform.NativeImage;
import cursed.recode.render.Native;
import java.nio.ByteBuffer;
import java.nio.IntBuffer;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.HashMap;
import java.util.Map;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.renderer.RenderPipelines;
import net.minecraft.client.renderer.texture.DynamicTexture;
import net.minecraft.resources.Identifier;
import net.minecraft.util.ARGB;
import org.lwjgl.PointerBuffer;
import org.lwjgl.stb.STBImage;
import org.lwjgl.system.MemoryStack;
import org.lwjgl.system.MemoryUtil;

/**
 * The player's pictures in the HUD (gui/hudx.rs, the icons of kind 7): each file read once with STB (PNG, JPG, BMP,
 * GIF with every frame and its delay) into textures of its own; a GIF plays at its element's speed. Each frame is kept
 * at its size and at halves of it (averaged four pixels to one), and drawn from the one nearest above the size it has on
 * screen, smoothly: a big cover small stays clean instead of breaking into dots; a small pixel picture keeps its pixels.
 */
public final class HudPictures {
    private HudPictures() {
    }

    /** frames[level][frame]: the levels each half the last; w and h of the first. */
    private record Pic(Identifier[][] frames, int[] delays, int total, int w, int h) {
    }

    /** Each element's number: its file and its GIF's speed. */
    private static final Map<Integer, String> SLOTS = new HashMap<>();
    private static final Map<Integer, Float> SPEEDS = new HashMap<>();
    private static final Map<String, Pic> PICS = new HashMap<>();
    private static final Map<String, Boolean> FAILED = new HashMap<>();
    private static long polledAt;
    private static int seq;

    /** The elements' files (a few times a second). */
    private static void poll() {
        long now = System.nanoTime();
        if (now - polledAt < 400_000_000L) {
            return;
        }
        polledAt = now;
        SLOTS.clear();
        String s = Native.guiCall("images");
        if (s == null || s.isEmpty()) {
            return;
        }
        for (String line : s.split("\n")) {
            String[] f = line.split("\t");
            if (f.length < 2) {
                continue;
            }
            try {
                int n = Integer.parseInt(f[0]);
                // the cover of what plays (-1): its file is reused for the next one, its number says which
                SLOTS.put(n, f.length > 3 ? f[1] + "#" + f[3] : f[1]);
                SPEEDS.put(n, f.length > 2 ? Float.parseFloat(f[2]) : 1.0F);
            } catch (NumberFormatException ignored) {
            }
        }
    }

    private static @org.jspecify.annotations.Nullable String coverKey;

    private static Pic load(String key) {
        Pic p = PICS.get(key);
        if (p != null || FAILED.containsKey(key)) {
            return p;
        }
        String path = key.contains("#") ? key.substring(0, key.lastIndexOf('#')) : key;
        if (key.contains("#")) {
            // a new cover: the last one's textures go
            if (coverKey != null) {
                Pic old = PICS.remove(coverKey);
                if (old != null) {
                    for (Identifier[] level : old.frames) {
                        for (Identifier id : level) {
                            Minecraft.getInstance().getTextureManager().release(id);
                        }
                    }
                }
            }
            coverKey = key;
        }
        ByteBuffer buf = null;
        try {
            byte[] bytes = Files.readAllBytes(Path.of(path));
            buf = MemoryUtil.memAlloc(bytes.length).put(bytes).flip();
            try (MemoryStack st = MemoryStack.stackPush()) {
                IntBuffer w = st.mallocInt(1), h = st.mallocInt(1), comp = st.mallocInt(1), count = st.mallocInt(1);
                ByteBuffer data;
                int frames = 1;
                int[] delays = {100};
                boolean gif = path.toLowerCase(java.util.Locale.ROOT).endsWith(".gif");
                if (gif) {
                    PointerBuffer dp = st.mallocPointer(1);
                    data = STBImage.stbi_load_gif_from_memory(buf, dp, w, h, count, comp, 4);
                    if (data != null) {
                        frames = Math.max(1, count.get(0));
                        IntBuffer d = MemoryUtil.memIntBuffer(dp.get(0), frames);
                        delays = new int[frames];
                        for (int i = 0; i < frames; i++) {
                            delays[i] = Math.max(20, d.get(i));
                        }
                        MemoryUtil.nmemFree(dp.get(0));
                    }
                } else {
                    data = STBImage.stbi_load_from_memory(buf, w, h, comp, 4);
                }
                if (data == null) {
                    FAILED.put(key, true);
                    return null;
                }
                int W = w.get(0), H = h.get(0);
                long frameBytes = (long) W * H * 4;
                // as many halves as keep it at least 16 px (and none for a small one: its pixels are its look)
                int levels = 1;
                while ((W >> levels) >= 16 && (H >> levels) >= 16 && levels < 8 && Math.max(W, H) > 64) {
                    levels++;
                }
                // a GIF's frames all at every level would be heavy: a long one keeps fewer
                if ((long) frames * frameBytes > 64L << 20) {
                    levels = 1;
                }
                Identifier[][] ids = new Identifier[levels][frames];
                int n0 = ++seq;
                boolean smooth = Math.max(W, H) > 64;
                for (int i = 0; i < frames; i++) {
                    byte[] px = new byte[(int) frameBytes];
                    MemoryUtil.memByteBuffer(MemoryUtil.memAddress(data) + i * frameBytes, (int) frameBytes).get(px);
                    int lw = W, lh = H;
                    for (int l = 0; l < levels; l++) {
                        if (l > 0) {
                            px = half(px, lw, lh);
                            lw = Math.max(1, lw / 2);
                            lh = Math.max(1, lh / 2);
                        }
                        NativeImage img = new NativeImage(NativeImage.Format.RGBA, lw, lh, false);
                        MemoryUtil.memByteBuffer(img.getPointer(), px.length).put(px);
                        ids[l][i] = Identifier.fromNamespaceAndPath("cursed-recode", "hud-pic/" + n0 + "/" + l + "/" + i);
                        Minecraft.getInstance().getTextureManager().register(ids[l][i], texture(img, smooth));
                    }
                }
                STBImage.stbi_image_free(data);
                int total = 0;
                for (int d : delays) {
                    total += d;
                }
                p = new Pic(ids, delays, Math.max(1, total), W, H);
                PICS.put(key, p);
                Native.guiCall("picsize\t" + path + "\t" + W + "\t" + H);
                return p;
            }
        } catch (Exception e) {
            FAILED.put(key, true);
            return null;
        } finally {
            if (buf != null) {
                MemoryUtil.memFree(buf);
            }
        }
    }

    /** A frame's pixels (RGBA) at half its size, each four averaged (by their alpha, so edges keep no dark fringe). */
    private static byte[] half(byte[] src, int w, int h) {
        int nw = Math.max(1, w / 2), nh = Math.max(1, h / 2);
        byte[] out = new byte[nw * nh * 4];
        for (int y = 0; y < nh; y++) {
            for (int x = 0; x < nw; x++) {
                int r = 0, g = 0, b = 0, a = 0;
                for (int k = 0; k < 4; k++) {
                    int sx = Math.min(w - 1, x * 2 + (k & 1)), sy = Math.min(h - 1, y * 2 + (k >> 1));
                    int o = (sy * w + sx) * 4;
                    int pa = src[o + 3] & 255;
                    r += (src[o] & 255) * pa;
                    g += (src[o + 1] & 255) * pa;
                    b += (src[o + 2] & 255) * pa;
                    a += pa;
                }
                int o = (y * nw + x) * 4;
                if (a > 0) {
                    out[o] = (byte) (r / a);
                    out[o + 1] = (byte) (g / a);
                    out[o + 2] = (byte) (b / a);
                }
                out[o + 3] = (byte) ((a + 2) / 4);
            }
        }
        return out;
    }

    /** A frame's texture: sampled smoothly (a photo, a cover) or by its pixels (a small picture). */
    private static DynamicTexture texture(NativeImage img, boolean smooth) {
        return new DynamicTexture(() -> "Cursed HUD picture", img) {
            {
                this.sampler = com.mojang.blaze3d.systems.RenderSystem.getSamplerCache()
                    .getClampToEdge(smooth ? com.mojang.renderpearl.api.textures.FilterMode.LINEAR : com.mojang.renderpearl.api.textures.FilterMode.NEAREST);
            }
        };
    }

    /** A picture where the HUD left room for it (its element's number), in GUI px. */
    static void draw(GuiGraphicsExtractor graphics, int slot, float x, float y, float w, float h, float alpha) {
        poll();
        String path = SLOTS.get(slot);
        if (path == null) {
            return;
        }
        Pic p = load(path);
        if (p == null) {
            return;
        }
        int f = 0;
        int count = p.frames[0].length;
        if (count > 1) {
            long t = (long) (System.nanoTime() / 1_000_000L * SPEEDS.getOrDefault(slot, 1.0F)) % p.total;
            int acc = 0;
            for (int i = 0; i < count; i++) {
                acc += p.delays[i];
                if (t < acc) {
                    f = i;
                    break;
                }
            }
        }
        // the level nearest above its size on screen
        var win = Minecraft.getInstance().getWindow();
        float px = w * win.getWidth() / (float) Math.max(1, win.getGuiScaledWidth());
        int level = 0;
        while (level + 1 < p.frames.length && (p.w >> (level + 1)) >= px) {
            level++;
        }
        int lw = Math.max(1, p.w >> level), lh = Math.max(1, p.h >> level);
        // the cover (-1) fills its square: a wide picture (a video's) is cut to its middle, never squeezed
        int cw = lw, ch = lh;
        if (slot == -1 && w > 0 && h > 0) {
            float want = w / h;
            if ((float) lw / lh > want) {
                cw = Math.max(1, Math.round(lh * want));
            } else {
                ch = Math.max(1, Math.round(lw / want));
            }
        }
        var pose = graphics.pose();
        pose.pushMatrix();
        pose.translate(x, y);
        pose.scale(w / cw, h / ch);
        graphics.blit(RenderPipelines.GUI_TEXTURED, p.frames[level][f], 0, 0, (lw - cw) / 2.0F, (lh - ch) / 2.0F, cw, ch, lw, lh, ARGB.white(alpha));
        pose.popMatrix();
    }
}
