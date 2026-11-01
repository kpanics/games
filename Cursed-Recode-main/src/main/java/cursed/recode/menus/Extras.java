package cursed.recode.menus;

import cursed.recode.render.Native;
import java.util.List;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.PlayerFaceExtractor;
import net.minecraft.world.entity.player.PlayerSkin;
import net.minecraft.world.item.ItemStack;
import net.minecraft.util.ARGB;
import org.jspecify.annotations.Nullable;

/**
 * What the game itself draws over a page where the page says (items, players' faces): the page's list of them is
 * numbered, the menus say which goes where each frame (menus/mod.rs extras: kind, number, x, y, size, alpha; window px).
 */
final class Extras {
    static final int ITEM = 1, FACE = 2, ALT_FACE = 3, ALT_CLIP = 4;
    private static final float[] BUF = new float[6 * 512];
    /**
     * Whose places are drawn now: a screen's number (each place is marked with the screen that said it, in the
     * thousands of its kind), 0 for all. A page's numbers are its own: another's would draw the wrong things.
     */
    static int page;

    private Extras() {
    }

    /** The places said this frame, each to `draw` (kind, number, x, y, size, alpha in window px). */
    static void each(Drawer draw) {
        int n = Native.menusExtras(BUF);
        for (int i = 0; i < n; i++) {
            int code = (int) BUF[i * 6];
            if (mine(code)) {
                draw.at(code % 1000, (int) BUF[i * 6 + 1], BUF[i * 6 + 2], BUF[i * 6 + 3], BUF[i * 6 + 4], BUF[i * 6 + 5]);
            }
        }
    }

    interface Drawer {
        void at(int kind, int ref, float x, float y, float size, float alpha);
    }

    /** The places said this frame with their numbers as said (a fraction can carry more: how far an item turns). */
    static void eachRaw(RawDrawer draw) {
        int n = Native.menusExtras(BUF);
        for (int i = 0; i < n; i++) {
            int code = (int) BUF[i * 6];
            if (mine(code)) {
                draw.at(code % 1000, BUF[i * 6 + 1], BUF[i * 6 + 2], BUF[i * 6 + 3], BUF[i * 6 + 4], BUF[i * 6 + 5]);
            }
        }
    }

    private static boolean mine(int code) {
        return page == 0 || code / 1000 == page % 16000;
    }

    interface RawDrawer {
        void at(int kind, float number, float x, float y, float size, float alpha);
    }

    /** An item at window px (`size` its side), fading for real: the menus' item stage, else the game's way. */
    static void item(GuiGraphicsExtractor g, @Nullable ItemStack stack, float x, float y, float size, float alpha) {
        if (stack != null && ItemStage.MENUS.add(g, stack, x, y, size, 0.0F, 0.0F, alpha)) {
            return;
        }
        gameItem(g, stack, x, y, size, alpha);
    }

    /**
     * An item at window px (`size` its side) drawn by the game, which draws it whole — fading it out makes it smaller
     * instead (as the HUD's icons). For what is drawn in another layer than the page (the toasts over the HUD).
     */
    static void gameItem(GuiGraphicsExtractor g, @Nullable ItemStack stack, float x, float y, float size, float alpha) {
        if (stack == null || stack.isEmpty() || alpha <= 0.02F) {
            return;
        }
        double s = Minecraft.getInstance().getWindow().getGuiScale();
        float k = size / 16.0F / (float) s * Math.min(1.0F, 0.4F + 0.6F * alpha);
        float cx = (x + size / 2.0F) / (float) s, cy = (y + size / 2.0F) / (float) s;
        g.pose().pushMatrix();
        g.pose().translate(cx, cy);
        g.pose().scale(k, k);
        g.fakeItem(stack, -8, -8);
        g.pose().popMatrix();
    }

    /**
     * Alt Manager's card (native menus/alts.rs: kind 3, the account's place in its list): each account's face where the
     * card says — the main menu's (no page: all of this frame's) and the pause's — cut to the list's part as it scrolls
     * (kind 4: its left, top, right in x, y, size, its bottom in the number).
     */
    public static void altFaces(GuiGraphicsExtractor g) {
        Minecraft mc = Minecraft.getInstance();
        List<String[]> list = null;
        int n = Native.menusExtras(BUF);
        float[] clip = null;
        for (int i = 0; i < n; i++) {
            int code = (int) BUF[i * 6];
            if (code % 1000 == ALT_CLIP && mine(code)) {
                clip = new float[] {BUF[i * 6 + 2], BUF[i * 6 + 3], BUF[i * 6 + 4], BUF[i * 6 + 1]};
            }
        }
        double s = mc.getWindow().getGuiScale();
        for (int i = 0; i < n; i++) {
            int code = (int) BUF[i * 6];
            if (code % 1000 != ALT_FACE || !mine(code)) {
                continue;
            }
            if (list == null) {
                list = cursed.recode.alts.Alts.listed();
            }
            int ref = (int) BUF[i * 6 + 1];
            float y = BUF[i * 6 + 3], size = BUF[i * 6 + 4];
            if (ref < 0 || ref >= list.size() || clip != null && (y >= clip[3] || y + size <= clip[1])) {
                continue;
            }
            boolean whole = clip == null || y >= clip[1] && y + size <= clip[3];
            if (!whole) {
                g.enableScissor((int) Math.ceil(clip[0] / s), (int) Math.ceil(clip[1] / s), (int) Math.floor(clip[2] / s), (int) Math.floor(clip[3] / s));
            }
            face(g, cursed.recode.alts.Alts.skin(mc, list.get(ref)[0], list.get(ref)[1]), BUF[i * 6 + 2], y, size, BUF[i * 6 + 5]);
            if (!whole) {
                g.disableScissor();
            }
        }
    }

    static void face(GuiGraphicsExtractor g, @Nullable PlayerSkin skin, float x, float y, float size, float alpha) {
        if (skin == null || alpha <= 0.02F) {
            return;
        }
        double s = Minecraft.getInstance().getWindow().getGuiScale();
        PlayerFaceExtractor.extractRenderState(g, skin, Math.round(x / (float) s), Math.round(y / (float) s), Math.max(1, Math.round(size / (float) s)), ARGB.white(alpha));
    }

    /**
     * Items that turn (kind 8: the number's fraction is how far round), real models in the menus' stage, tilted a
     * little toward the viewer; without the stage, still.
     */
    static void turning(GuiGraphicsExtractor g, List<ItemStack> stacks) {
        eachRaw((kind, number, x, y, size, alpha) -> {
            int ref = (int) Math.floor(number);
            if (kind != 8 || ref < 0 || ref >= stacks.size()) {
                return;
            }
            if (!ItemStage.MENUS.add(g, stacks.get(ref), x, y, size, (number - ref) * 360.0F, -12.0F, alpha)) {
                gameItem(g, stacks.get(ref), x, y, size, alpha);
            }
        });
    }

    /**
     * Creatures as the inventory draws the player (kind 9: the number's fraction is how far round, 0 facing you),
     * seen a little from above, in the menus' stage (not without a world).
     */
    static void mobs(GuiGraphicsExtractor g, List<net.minecraft.world.entity.EntityType<?>> types) {
        eachRaw((kind, number, x, y, size, alpha) -> {
            int ref = (int) Math.floor(number);
            if (kind == 9 && ref >= 0 && ref < types.size()) {
                ItemStage.MENUS.addMob(g, types.get(ref), x, y, size, (number - ref) * 360.0F, 8.0F, alpha);
            }
        });
    }

    static void items(GuiGraphicsExtractor g, List<ItemStack> stacks) {
        each((kind, ref, x, y, size, alpha) -> {
            if (kind == ITEM && ref >= 0 && ref < stacks.size()) {
                item(g, stacks.get(ref), x, y, size, alpha);
            }
        });
    }
}
