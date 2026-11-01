package cursed.recode.hud;

import com.mojang.blaze3d.platform.NativeImage;
import com.mojang.blaze3d.systems.RenderSystem;
import com.mojang.renderpearl.api.GpuFormat;
import com.mojang.renderpearl.api.textures.FilterMode;
import com.mojang.renderpearl.api.textures.GpuTexture;
import com.mojang.renderpearl.api.textures.GpuTextureView;
import cursed.recode.render.GuiFrame;
import cursed.recode.render.Native;
import java.io.InputStream;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.inventory.AbstractContainerScreen;
import net.minecraft.client.renderer.texture.DynamicTexture;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.Style;
import net.minecraft.resources.Identifier;
import net.minecraft.util.Mth;
import net.minecraft.world.inventory.Slot;
import org.jspecify.annotations.Nullable;

/**
 * Custom Gui (Interface; native gui/hudscreen.rs): every container screen of the game (the inventory, chests, furnaces,
 * villagers, the anvil, the enchanting table, the beacon…) on the client's glass. One way for all of them: the game's
 * background picture is taken without its grey — the panel's fill, its bevels and outline, the slots' frames (the
 * slots stay as soft wells of the client's colour) — and everything else in it stays as it was (arrows, flames, icons,
 * the beacon's pyramid); under it the HUD draws the glass panel and the hovered slot's light that slides from slot to
 * slot; the dark words become light; tooltips on the client's plate; the screen grows in as it opens. A server's menu
 * drawn with its own pictures (its title in a pack's glyphs) is left as the server made it.
 */
public final class HudScreen {
    private HudScreen() {
    }

    private static boolean on, ownTooltips = true, keepServer = true;
    private static int wellArgb = 0x14FFFFFF, labelRgb = 0xECECEC, dimAlpha = 90, opening;
    private static float openSpeed = 1.0F;

    /** In the extraction of a container screen the Custom Gui draws (the words, the pictures, the slot lights). */
    private static boolean drawing;
    private static @Nullable Screen shown;
    private static boolean server;
    private static long openAt;
    private static boolean posed;
    private static int panelId;

    /** The game's pictures without their grey, by the picture (made once, again when the wells' colour changes). */
    private static final Map<Identifier, Identifier> RECOLOURED = new HashMap<>();
    private static int madeFor = -1;
    /** The resources reloaded (packs, F3+T): the pictures are made again from what is there now. */
    private static boolean stale;

    public static void reloading() {
        stale = true;
    }

    private static @Nullable GpuTexture texture;
    private static @Nullable GpuTextureView view;
    /** This frame: the panel is under the picture already (a recipe book's screen draws its background twice). */
    private static boolean panelDone;

    /** From the HUD's state (Native.hudState: 70 on). */
    static void state(float[] s) {
        on = s[70] > 0.5F;
        int wa = Mth.clamp(Math.round(s[71] * 255.0F), 0, 255);
        wellArgb = wa << 24 | ((int) s[72] & 0xFFFFFF);
        labelRgb = (int) s[73] & 0xFFFFFF;
        dimAlpha = Mth.clamp(Math.round(s[74] * 255.0F), 0, 255);
        opening = (int) s[75];
        openSpeed = Math.max(0.25F, s[76]);
        keepServer = s[77] > 0.5F;
        ownTooltips = s[78] > 0.5F;
    }

    /** The Custom Gui draws this screen (a container screen, not a server's own picture menu). */
    public static boolean active(@Nullable Screen screen) {
        // (the survival and the creative inventories may be Custom Inventory's)
        if (!on || !(screen instanceof AbstractContainerScreen<?>) || !GuiFrame.active() || HudInv.active(screen) || HudCreative.active(screen) || HudPack.active(screen)) {
            return false;
        }
        if (screen != shown) {
            shown = screen;
            openAt = System.nanoTime();
            server = keepServer && serverMade(screen.getTitle());
        }
        return !server;
    }

    /** A title in a resource pack's glyphs: the server's picture menu. A font of its own, a glyph where packs put
     *  theirs (HudText.glyph: private use characters, their planes, the Yi block), or any letter of the game's own font
     *  a pack made a picture or a space of — far wider than a letter, or going back. */
    static boolean serverMade(Component title) {
        var font = Minecraft.getInstance().font;
        boolean[] found = {false};
        HudText.pieces(title, (style, text) -> {
            if (found[0]) {
                return;
            }
            if (!style.getFont().equals(net.minecraft.network.chat.FontDescription.DEFAULT)) {
                found[0] = true;
                return;
            }
            for (int i = 0; i < text.length() && !found[0]; ) {
                int cp = text.codePointAt(i);
                i += Character.charCount(cp);
                if (HudText.glyph(cp)) {
                    found[0] = true;
                } else if (cp > 0x7F) {
                    int w = font.width(net.minecraft.network.chat.FormattedText.of(new String(Character.toChars(cp)), style));
                    found[0] = w < 0 || w > 24;
                }
            }
        });
        return found[0];
    }

    /** Whether a server's picture menu is left as the server made it (Custom Gui's "Keep server menus"). */
    static boolean keepsServer() {
        return keepServer;
    }

    public static boolean drawing() {
        return drawing;
    }

    // ------------------------------------------------------------------------------------------ the HUD's facts

    /** The open container for the HUD (gui/hudscreen.rs Screen::parse): where its picture is, its slots, the hovered. */
    static void append(Minecraft mc, List<Double> n) {
        Screen s = mc.gui.screen();
        if (!(s instanceof AbstractContainerScreen<?> cs) || !active(s)) {
            n.add(0.0);
            return;
        }
        var acc = (cursed.recode.mixin.client.ContainerScreenAccess) cs;
        float gs = (float) mc.getWindow().getGuiScale();
        Slot hovered = acc.cursed$hovered();
        var slots = cs.getMenu().slots;
        n.add(1.0);
        n.add((double) gs);
        n.add((double) acc.cursed$left());
        n.add((double) acc.cursed$top());
        n.add((double) acc.cursed$width());
        n.add((double) acc.cursed$height());
        n.add(hovered != null && hovered.isActive() && hovered.isHighlightable() ? (double) hovered.x : -999.0);
        n.add(hovered != null && hovered.isActive() && hovered.isHighlightable() ? (double) hovered.y : -999.0);
        n.add((System.nanoTime() - openAt) / 1e9);
        int count = 0;
        for (Slot sl : slots) {
            if (sl.isActive()) {
                count++;
            }
        }
        count = Math.min(count, 128);
        n.add((double) count);
        int k = 0;
        for (Slot sl : slots) {
            if (sl.isActive() && k++ < count) {
                n.add((double) sl.x);
                n.add((double) sl.y);
            }
        }
    }

    // ------------------------------------------------------------------------------------------ in the screen

    /** The screen's extraction begins (its words, pictures and slots are the Custom Gui's until it ends). */
    public static void begin(Screen screen) {
        drawing = active(screen);
        posed = false;
        panelDone = false;
    }

    /** It ends: what grew in is put back. */
    public static void end(GuiGraphicsExtractor graphics) {
        if (posed) {
            posed = false;
            graphics.pose().popMatrix();
        }
        drawing = false;
    }

    /** After the dimmed world: the screen grows in, the HUD's panel goes under the game's picture. */
    public static void afterBackground(Screen screen, GuiGraphicsExtractor graphics) {
        if (!drawing || panelDone || screen != shown) {
            return;
        }
        panelDone = true;
        float t = (System.nanoTime() - openAt) / 1e9F * openSpeed;
        float e = ease(t / 0.24F);
        graphics.pose().pushMatrix();
        posed = true;
        float cx = screen.width / 2.0F, cy = screen.height / 2.0F;
        switch (opening) {
            case 0 -> {
                float k = 0.94F + 0.06F * e;
                graphics.pose().translate(cx, cy);
                graphics.pose().scale(k, k);
                graphics.pose().translate(-cx, -cy);
            }
            case 1 -> graphics.pose().translate(0.0F, (1.0F - e) * 10.0F);
            default -> {
            }
        }
        int id = Native.hudScreen();
        if (id == 0) {
            return;
        }
        var win = Minecraft.getInstance().getWindow();
        int w = win.getWidth(), h = win.getHeight();
        if (texture == null || texture.getWidth(0) != w || texture.getHeight(0) != h) {
            if (view != null) {
                view.close();
            }
            if (texture != null) {
                texture.close();
            }
            texture = RenderSystem.getDevice().createTexture("Cursed screen", GpuTexture.USAGE_COPY_DST | GpuTexture.USAGE_TEXTURE_BINDING, GpuFormat.RGBA8_UNORM, w, h, 1, 1);
            view = RenderSystem.getDevice().createTextureView(texture);
        }
        GuiFrame.send(texture, id);
        graphics.blit(view, RenderSystem.getSamplerCache().getClampToEdge(FilterMode.NEAREST), 0, 0, graphics.guiWidth(), graphics.guiHeight(), 0.0F, 1.0F, 0.0F, 1.0F);
        // the player's pictures shown over the inventories
        HudLayer.screenIcons(Minecraft.getInstance(), graphics);
    }

    private static float ease(float x) {
        float t = Mth.clamp(x, 0.0F, 1.0F);
        return 1.0F - (1.0F - t) * (1.0F - t) * (1.0F - t);
    }

    /** The dimming of the world behind (its gradient's two colours). */
    public static int dim(int argb, boolean top) {
        return drawing ? (dimAlpha << 24) | (top ? 0x08080C : 0x040406) : argb;
    }

    /** A dark word (the game's labels, 0x404040, and the like) as a light one. */
    public static int words(int argb) {
        if (!drawing) {
            return argb;
        }
        int r = argb >> 16 & 255, g = argb >> 8 & 255, b = argb & 255;
        int max = Math.max(r, Math.max(g, b)), min = Math.min(r, Math.min(g, b));
        if (max < 0x70 && max - min < 0x18) {
            return (argb & 0xFF000000) | labelRgb;
        }
        return argb;
    }

    /** The game's sprites of containers drawn on grey that have a dark one of ours (assets/cursed/textures/gui/sprites:
     *  made from the game's by the same rule, the plates dark instead of gone): effects, creative tabs, the beacon's
     *  buttons, text fields, scroll bars… */
    private static final java.util.Set<String> OWN_SPRITES = java.util.Set.of(
            "container/beacon/button",
            "container/beacon/button_disabled",
            "container/beacon/button_selected",
            "container/creative_inventory/scroller",
            "container/creative_inventory/scroller_disabled",
            "container/creative_inventory/tab_bottom_selected_1",
            "container/creative_inventory/tab_bottom_selected_2",
            "container/creative_inventory/tab_bottom_selected_3",
            "container/creative_inventory/tab_bottom_selected_4",
            "container/creative_inventory/tab_bottom_selected_5",
            "container/creative_inventory/tab_bottom_selected_6",
            "container/creative_inventory/tab_bottom_selected_7",
            "container/creative_inventory/tab_bottom_unselected_1",
            "container/creative_inventory/tab_bottom_unselected_2",
            "container/creative_inventory/tab_bottom_unselected_3",
            "container/creative_inventory/tab_bottom_unselected_4",
            "container/creative_inventory/tab_bottom_unselected_5",
            "container/creative_inventory/tab_bottom_unselected_6",
            "container/creative_inventory/tab_bottom_unselected_7",
            "container/creative_inventory/tab_top_selected_1",
            "container/creative_inventory/tab_top_selected_2",
            "container/creative_inventory/tab_top_selected_3",
            "container/creative_inventory/tab_top_selected_4",
            "container/creative_inventory/tab_top_selected_5",
            "container/creative_inventory/tab_top_selected_6",
            "container/creative_inventory/tab_top_selected_7",
            "container/creative_inventory/tab_top_unselected_1",
            "container/creative_inventory/tab_top_unselected_2",
            "container/creative_inventory/tab_top_unselected_3",
            "container/creative_inventory/tab_top_unselected_4",
            "container/creative_inventory/tab_top_unselected_5",
            "container/creative_inventory/tab_top_unselected_6",
            "container/creative_inventory/tab_top_unselected_7",
            "container/horse/chest_slots",
            "container/inventory/effect_background",
            "container/inventory/effect_background_ambient",
            "container/loom/scroller",
            "container/loom/scroller_disabled",
            "container/stonecutter/scroller",
            "container/stonecutter/scroller_disabled",
            "container/villager/scroller",
            "container/villager/scroller_disabled",
            "recipe_book/overlay_recipe",
            "recipe_book/tab",
            "recipe_book/tab_selected",
            "widget/scroller_background",
            "widget/text_field",
            "widget/text_field_highlighted",
            "container/beacon/button_highlighted",
            "container/anvil/text_field",
            "container/anvil/text_field_disabled",
            "container/enchanting_table/enchantment_slot",
            "container/enchanting_table/enchantment_slot_disabled",
            "container/enchanting_table/enchantment_slot_highlighted",
            "container/stonecutter/recipe",
            "container/stonecutter/recipe_highlighted",
            "container/stonecutter/recipe_selected",
            "container/loom/pattern",
            "container/loom/pattern_highlighted",
            "container/loom/pattern_selected",
            "widget/button",
            "widget/button_highlighted",
            "widget/button_disabled",
            "recipe_book/button",
            "recipe_book/button_highlighted",
            "recipe_book/filter_disabled",
            "recipe_book/filter_disabled_highlighted",
            "recipe_book/filter_enabled",
            "recipe_book/filter_enabled_highlighted",
            "recipe_book/furnace_filter_disabled",
            "recipe_book/furnace_filter_disabled_highlighted",
            "recipe_book/furnace_filter_enabled",
            "recipe_book/furnace_filter_enabled_highlighted");

    /** A sprite of ours in place of the game's grey one, in a container the Custom Gui draws. */
    public static Identifier sprite(Identifier id) {
        if (drawing && "minecraft".equals(id.getNamespace()) && OWN_SPRITES.contains(id.getPath())) {
            return Identifier.fromNamespaceAndPath("cursed", id.getPath());
        }
        return id;
    }

    /** The tooltip's plate: the client's (a style of the game's or a server's is left as it is). */
    public static @Nullable Identifier tooltip(@Nullable Identifier style, boolean frame) {
        if (!drawing || !ownTooltips || style != null) {
            return null;
        }
        return Identifier.fromNamespaceAndPath("cursed", frame ? "tooltip/frame" : "tooltip/background");
    }

    // ------------------------------------------------------------------------------------------ the pictures

    /** A container's picture without its grey (made the first time it is asked for). */
    public static Identifier picture(Identifier id) {
        if (!drawing || !"minecraft".equals(id.getNamespace()) || !id.getPath().startsWith("textures/gui/container/")) {
            return id;
        }
        if (madeFor != wellArgb || stale) {
            stale = false;
            Minecraft mc = Minecraft.getInstance();
            for (Identifier made : RECOLOURED.values()) {
                if (made != null && !made.equals(id)) {
                    mc.getTextureManager().release(made);
                }
            }
            RECOLOURED.clear();
            madeFor = wellArgb;
        }
        Identifier made = RECOLOURED.get(id);
        if (made == null) {
            made = make(id);
            RECOLOURED.put(id, made);
        }
        return made;
    }

    private static Identifier make(Identifier id) {
        Minecraft mc = Minecraft.getInstance();
        var res = mc.getResourceManager().getResource(id);
        // (a resource pack's own picture of the container is left as its author drew it)
        if (res.isEmpty() || !"vanilla".equals(res.get().sourcePackId())) {
            return id;
        }
        try (InputStream in = res.get().open(); NativeImage src = NativeImage.read(in)) {
            int w = src.getWidth(), h = src.getHeight();
            NativeImage out = new NativeImage(w, h, true);
            for (int y = 0; y < h; y++) {
                for (int x = 0; x < w; x++) {
                    out.setPixel(x, y, recolour(src, x, y, w, h));
                }
            }
            Identifier made = Identifier.fromNamespaceAndPath("cursed", "custom_gui/" + id.getPath().replace('/', '_'));
            String name = made.toString();
            mc.getTextureManager().register(made, new DynamicTexture(() -> name, out));
            return made;
        } catch (Exception e) {
            return id;
        }
    }

    /** One pixel: the panel's grey, its bevels and the slots' frames gone, the slots' fill the wells' colour, the
     *  outline gone where it borders nothing, the inner black and the beacon's dark softened; the rest as it was. */
    private static int recolour(NativeImage src, int x, int y, int w, int h) {
        int p = src.getPixel(x, y);
        if ((p >>> 24) == 0) {
            return 0;
        }
        int rgb = p & 0xFFFFFF;
        return switch (rgb) {
            // the panel, its bevels, the slots' frames; the tan lists' frames (the stonecutter, the loom, the
            // enchanting table) and the anvil's red under its field
            case 0xC6C6C6, 0xFFFFFF, 0x555555, 0x373737, 0x6B614C, 0x544C3B, 0xE0CA9F, 0xFF0000 -> 0;
            case 0x8B8B8B, 0xA09172 -> wellArgb;
            case 0x51493A -> 0x46000000;
            case 0x212121 -> 0x46000000;
            case 0x000000 -> edge(src, x, y, w, h) ? 0 : 0x5A000000;
            default -> p;
        };
    }

    private static boolean edge(NativeImage src, int x, int y, int w, int h) {
        for (int dy = -1; dy <= 1; dy++) {
            for (int dx = -1; dx <= 1; dx++) {
                int nx = x + dx, ny = y + dy;
                if (nx < 0 || ny < 0 || nx >= w || ny >= h || (src.getPixel(nx, ny) >>> 24) == 0) {
                    return true;
                }
            }
        }
        return false;
    }
}
