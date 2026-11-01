package cursed.recode.hud;

import cursed.recode.mixin.client.ContainerScreenAccess;
import cursed.recode.render.GuiFrame;
import cursed.recode.render.Native;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.Hud;
import net.minecraft.client.gui.components.ImageButton;
import net.minecraft.client.gui.components.Renderable;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.inventory.AbstractContainerScreen;
import net.minecraft.client.gui.screens.inventory.AbstractFurnaceScreen;
import net.minecraft.client.gui.screens.inventory.AbstractMountInventoryScreen;
import net.minecraft.client.gui.screens.inventory.AnvilScreen;
import net.minecraft.client.gui.screens.inventory.BrewingStandScreen;
import net.minecraft.client.gui.screens.inventory.CartographyTableScreen;
import net.minecraft.client.gui.screens.inventory.ContainerScreen;
import net.minecraft.client.gui.screens.inventory.CrafterScreen;
import net.minecraft.client.gui.screens.inventory.CraftingScreen;
import net.minecraft.client.gui.screens.inventory.DispenserScreen;
import net.minecraft.client.gui.screens.inventory.GrindstoneScreen;
import net.minecraft.client.gui.screens.inventory.HopperScreen;
import net.minecraft.client.gui.screens.inventory.InventoryScreen;
import net.minecraft.client.gui.screens.inventory.ShulkerBoxScreen;
import net.minecraft.client.gui.screens.inventory.SmithingScreen;
import net.minecraft.client.gui.screens.inventory.EnchantmentScreen;
import net.minecraft.client.gui.screens.inventory.StonecutterScreen;
import net.minecraft.client.gui.screens.inventory.MerchantScreen;
import net.minecraft.client.gui.screens.inventory.LoomScreen;
import net.minecraft.client.gui.screens.inventory.BeaconScreen;
import net.minecraft.world.inventory.EnchantmentMenu;
import net.minecraft.world.inventory.StonecutterMenu;
import net.minecraft.world.inventory.AbstractFurnaceMenu;
import net.minecraft.world.inventory.AnvilMenu;
import net.minecraft.world.inventory.BrewingStandMenu;
import net.minecraft.world.inventory.CrafterMenu;
import net.minecraft.world.inventory.SmithingMenu;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.client.renderer.RenderPipelines;
import net.minecraft.core.Holder;
import net.minecraft.network.chat.Component;
import net.minecraft.util.ARGB;
import net.minecraft.world.effect.MobEffect;
import net.minecraft.world.effect.MobEffectInstance;
import net.minecraft.world.inventory.AbstractContainerMenu;
import net.minecraft.world.inventory.CraftingContainer;
import net.minecraft.world.inventory.InventoryMenu;
import net.minecraft.world.inventory.ResultSlot;
import net.minecraft.world.inventory.Slot;
import net.minecraft.world.item.ItemStack;
import org.jspecify.annotations.Nullable;

/**
 * Custom Inventory (Interface; native gui/hud/hudpack.rs): the survival inventory and the containers as blocks of the
 * client's glass in the game's own arrangement (her choice, 15.10). The game's screen and menu stay what they are
 * (its slots, clicks, drags, the number keys, dropping, the packets) but nothing of it is drawn: this sends the native
 * what the game knows each frame, puts its picture under all, the player, the effects' icons, the items where it laid
 * the cells out, the carried stack, then its picture over them (counts, wear, the tooltip's card). The slot under the
 * pointer is the one under it in that layout. Anything going wrong: the game's own screen from then on.
 */
public final class HudPack {
    private HudPack() {
    }

    /** What a slot is to the layout (native hudpack::role). */
    private static final int HIDDEN = 0, BACKPACK = 1, HOTBAR = 2, ARMOUR = 3, OFFHAND = 4, CRAFT = 5, CRAFTED = 6, CONTAINER = 7, OUTPUT = 8;

    private static boolean on;
    private static boolean broken;
    private static @Nullable Screen shown;
    /** The screen shown is a server's picture menu (its title in a pack's glyphs): left as the server made it. */
    private static boolean server;
    private static long openAt;
    private static final float[] OUT = new float[8192];
    private static int outN;
    private static float gs = 1.0F;
    private static final List<MobEffectInstance> EFFECTS = new ArrayList<>();
    /** The effects' durations when first seen. */
    private static final Map<Holder<MobEffect>, Integer> FIRST = new HashMap<>();

    /** From the HUD's state (Native.hudState: 79 Custom Inventory on). */
    static void state(float[] s) {
        on = s[79] > 0.5F;
    }

    /** The client draws this screen: a container it lays out (the creative inventory is HudCreative's). */
    public static boolean active(@Nullable Screen screen) {
        Minecraft mc = Minecraft.getInstance();
        if (!on || broken || !GuiFrame.active() || mc.player == null || !supported(screen, mc.player)) {
            return false;
        }
        if (screen != shown) {
            shown = screen;
            openAt = System.nanoTime();
            FIRST.clear();
            held = 0;
            // (its picture is the menu: its buttons are drawn in it, the things in its slots invisible; Custom Gui's
            // "Keep server menus" says so for both)
            server = screen != null && HudScreen.keepsServer() && HudScreen.serverMade(screen.getTitle());
        }
        return !server;
    }

    private static boolean supported(@Nullable Screen s, LocalPlayer p) {
        return s instanceof InventoryScreen && !p.hasInfiniteMaterials() || s instanceof ContainerScreen || s instanceof ShulkerBoxScreen
                || s instanceof HopperScreen || s instanceof DispenserScreen || s instanceof AbstractFurnaceScreen<?> || s instanceof CraftingScreen
                || s instanceof BrewingStandScreen || s instanceof SmithingScreen || s instanceof GrindstoneScreen || s instanceof CartographyTableScreen
                || s instanceof AnvilScreen || s instanceof CrafterScreen || s instanceof AbstractMountInventoryScreen<?>
                || s instanceof EnchantmentScreen || s instanceof StonecutterScreen || s instanceof MerchantScreen || s instanceof LoomScreen
                || s instanceof BeaconScreen;
    }

    /** The screens whose own clicks (not the slots') the game works out from where its picture would be: a click over
     *  the container's block reaches them in the game's own px (MouseHandlerMixin, gameX/gameY). */
    private static boolean translated(@Nullable Screen s) {
        return s instanceof EnchantmentScreen || s instanceof StonecutterScreen || s instanceof MerchantScreen || s instanceof LoomScreen
                || s instanceof BeaconScreen;
    }

    /** A loom's patterns and a beacon's effects this frame (the pictures Java draws in their cells: pics 1 and 2). */
    private static final List<net.minecraft.core.Holder<net.minecraft.world.level.block.entity.BannerPattern>> PATTERNS = new ArrayList<>();
    private static final List<net.minecraft.core.Holder<MobEffect>> BEACON = new ArrayList<>();

    // the container's block as the game's px, from the last frame (OUT's frame): on for the screen it was made for
    private static @Nullable Screen frameOf;
    private static float fLeft, fTop, fK = 1.0F, fCx0, fCy0, fX0, fY0, fX1, fY1;

    /** The pointer's x (GUI px) as the game's screen would have it: over the container's block, in its own px. */
    public static double gameX(double scaled, double rawX, double rawY) {
        Minecraft mc = Minecraft.getInstance();
        Screen s = mc.gui.screen();
        if (s == null || s != frameOf || !translated(s) || rawX < fX0 || rawX >= fX1 || rawY < fY0 || rawY >= fY1) {
            return scaled;
        }
        int left = ((ContainerScreenAccess) s).cursed$left();
        return left + fCx0 + (rawX - fLeft) / fK;
    }

    public static double gameY(double scaled, double rawX, double rawY) {
        Minecraft mc = Minecraft.getInstance();
        Screen s = mc.gui.screen();
        if (s == null || s != frameOf || !translated(s) || rawX < fX0 || rawX >= fX1 || rawY < fY0 || rawY >= fY1) {
            return scaled;
        }
        int top = ((ContainerScreenAccess) s).cursed$top();
        return top + fCy0 + (rawY - fTop) / fK;
    }

    /** The items drawn besides the slots this frame (a stonecutter's recipes…). */
    private static final List<ItemStack> EXTRA = new ArrayList<>();

    /** Whose the clicks are, as the last frame said (native hudinvui Eats): every button's (a block held by its header,
     *  the settings' panel under the pointer), the left one's (a header), the right one's (a header, a block's empty
     *  place); the settings open (Esc closes them, not the screen). What is asked next frame (2: Esc). */
    private static boolean eatsAll, eatsLeft, eatsRight, panelOpen, panelKeys;
    private static int ask;
    /** The buttons held over the screen (1 the left, 2 the right: the game's own flags are kept only off screens),
     *  presses since the last frame (4, 8: a click quicker than a frame is not lost). */
    private static int held, pressed;

    /** A button pressed over the screen (before anything takes it). */
    public static void pressed(Screen screen, int button) {
        if (active(screen)) {
            held |= button == 0 ? 1 : button == 1 ? 2 : 0;
            pressed |= button == 0 ? 4 : button == 1 ? 8 : 0;
        }
    }

    /** A button let go over the screen. */
    public static void released(Screen screen, int button) {
        held &= ~(button == 0 ? 1 : button == 1 ? 2 : 0);
    }

    /** A press taken here (the editor's, the recipe book's): its drag and its release are ours as well (the game's
     *  release with a stack carried would put it down, or throw it). */
    private static boolean swallowed;

    /** A press over the screen: a header's, the settings', the recipe book's, or the game's. */
    public static boolean takeClick(Screen screen, net.minecraft.client.input.MouseButtonEvent event) {
        if (HudCreative.active(screen)) {
            return false;
        }
        int button = HudLayer.button(event.button());
        pressed(screen, button);
        boolean take = eats(screen, button) || recipesClick(screen, event);
        if (take) {
            swallowed = true;
        }
        return take;
    }

    public static boolean takeDrag(Screen screen) {
        if (HudCreative.active(screen)) {
            return HudCreative.takeDrag();
        }
        // (a drag and its release are the screen's own only when its press was: one the game began, let go over the
        // panel, is still the game's)
        return swallowed && active(screen);
    }

    public static boolean takeRelease(Screen screen, net.minecraft.client.input.MouseButtonEvent event) {
        if (HudCreative.active(screen)) {
            return HudCreative.takeRelease(HudLayer.button(event.button()));
        }
        released(screen, HudLayer.button(event.button()));
        boolean take = swallowed && active(screen);
        swallowed = false;
        return take;
    }

    /** Whether the settings' panel takes the keys and the letters (the colour wheel's hex being typed in it). */
    public static boolean panelKeys(@Nullable Screen screen) {
        return screen != null && (active(screen) && screen == frameOf && panelKeys || HudCreative.active(screen) && HudCreative.panelKeys());
    }

    /** A click on the recipe book (its button, its block): done there, not the game's. */
    public static boolean recipesClick(Screen screen, net.minecraft.client.input.MouseButtonEvent event) {
        if (!active(screen) || screen != frameOf || eatsAll) {
            return false;
        }
        Minecraft mc = Minecraft.getInstance();
        int button = HudLayer.button(event.button());
        return HudRecipes.click(mc, screen, (float) mc.mouseHandler.xpos(), (float) mc.mouseHandler.ypos(), button, event.hasShiftDown());
    }

    public static boolean recipesScroll(Screen screen, double dy) {
        if (!active(screen) || screen != frameOf) {
            return false;
        }
        // the settings under the pointer scroll
        if (panelOpen && Native.hudScroll((float) dy)) {
            return true;
        }
        Minecraft mc = Minecraft.getInstance();
        return HudRecipes.scroll((float) mc.mouseHandler.xpos(), (float) mc.mouseHandler.ypos(), dy);
    }

    /** A key or a letter while the recipe book's field has them. */
    public static boolean recipesKey(Screen screen, net.minecraft.client.input.KeyEvent event) {
        return active(screen) && HudRecipes.key(event.key(), event.isEscape(), event.key() == com.mojang.blaze3d.platform.InputConstants.KEY_RETURN);
    }

    public static boolean recipesTyped(Screen screen, int codepoint) {
        return active(screen) && HudRecipes.typed(codepoint);
    }

    /** A click (of `button`; -1 a drag, a release), the screen's own (a header's, the settings'): the game's screen does
     *  not get it. */
    private static boolean eats(Screen screen, int button) {
        return active(screen) && screen == frameOf && (eatsAll || button == 0 && eatsLeft || button == 1 && eatsRight);
    }

    /** Esc while the settings are open: they close (a list or the colour wheel first), the screen stays. */
    public static boolean escape(Screen screen) {
        if (HudCreative.active(screen)) {
            return HudCreative.escape();
        }
        if (active(screen) && screen == frameOf && panelOpen) {
            ask = 2;
            return true;
        }
        return false;
    }
    /** Lines of a tooltip of the screen's own (an enchantment's clue, a recipe's result), instead of a slot's. */
    private static @Nullable List<Component> ownTip;

    /** What a decoration is (native hudpack::deco). */
    private static final int FIRE = 1, ARROW = 2, BREW = 3, FUEL = 4, BUBBLES = 5, ERROR = 6, FIELD = 7, LABEL = 8, ENTITY = 9, POWER = 10, PLUS = 11, MAP = 12, OPTION = 13, BOOK = 14, GRID = 15, SCROLL = 16,
            TRADE = 17, XPBAR = 18, LOOM = 19, BANNER = 20, WIDGET = 21, ITEM = 22;

    /** The screen was laid out: the game's recipe book's button goes (its book is not there yet). */
    public static void init(Screen screen) {
        if (!active(screen)) {
            return;
        }
        List<Renderable> gone = new ArrayList<>();
        for (Renderable r : screen.renderables) {
            if (r instanceof ImageButton) {
                gone.add(r);
            }
        }
        for (Renderable r : gone) {
            if (r instanceof net.minecraft.client.gui.components.events.GuiEventListener l) {
                ((cursed.recode.mixin.client.ScreenInvoker) screen).cursed$remove(l);
            }
        }
    }

    // ------------------------------------------------------------------------------------------ the frame

    /** Instead of the game's whole screen (Screen.extractRenderStateWithTooltipAndSubtitles). */
    public static void render(AbstractContainerScreen<?> screen, GuiGraphicsExtractor graphics, int mouseX, int mouseY, float partial) {
        try {
            frame(screen, graphics, mouseX, mouseY, partial);
        } catch (Throwable t) {
            broken = true;
            org.slf4j.LoggerFactory.getLogger("HudPack").error("Custom Inventory failed; the game's own screens from now on", t);
        }
    }

    private static void frame(AbstractContainerScreen<?> screen, GuiGraphicsExtractor graphics, int mouseX, int mouseY, float partial) {
        Minecraft mc = Minecraft.getInstance();
        LocalPlayer p = mc.player;
        if (p == null) {
            return;
        }
        var win = mc.getWindow();
        int w = win.getWidth(), h = win.getHeight();
        if (w <= 0 || h <= 0) {
            return;
        }
        gs = (float) win.getGuiScale();
        var acc = (ContainerScreenAccess) screen;
        // the slot under the pointer, as the last frame laid them out (the clicks and keys go by it); the pointer as it
        // is (mouseX, mouseY are the game's px over a translated block)
        acc.cursed$setHovered(slotAt(screen, 0, 0));
        float rawX = (float) (mc.mouseHandler.xpos() / gs), rawY = (float) (mc.mouseHandler.ypos() / gs);
        StringBuilder text = new StringBuilder("pack\n");
        List<ItemStack> shownStacks = new ArrayList<>();
        double[] nums = facts(mc, p, screen, partial, text, shownStacks, mouseX, mouseY);
        int id = Native.invFrame(w, h, nums, text.toString());
        outN = Native.invOut(OUT);
        int top = Native.invTop();
        if (id == 0 || outN < 2) {
            return;
        }
        mc.gui.hud.extractDeferredSubtitles();
        HudInv.pictures(graphics, w, h, id, top);
        int r = (int) OUT[1];
        int o = 2 + r * 10;
        // the player, turning to the pointer as the game's does
        if (OUT[o + 2] > OUT[o]) {
            int x0 = Math.round(OUT[o] / gs), y0 = Math.round(OUT[o + 1] / gs), x1 = Math.round(OUT[o + 2] / gs), y1 = Math.round(OUT[o + 3] / gs);
            InventoryScreen.extractEntityInInventoryFollowsMouse(graphics, x0, y0, x1, y1, Math.max(1, Math.round(OUT[o + 4] / gs)), 0.0625F, mouseX, mouseY, p);
        }
        o += 5;
        // the effects' icons
        int e = (int) OUT[o++];
        for (int i = 0; i < e; i++, o += 5) {
            int k = (int) OUT[o];
            if (k >= 0 && k < EFFECTS.size() && OUT[o + 4] > 0.02F) {
                var pose = graphics.pose();
                pose.pushMatrix();
                pose.translate(OUT[o + 1] / gs, OUT[o + 2] / gs);
                pose.scale(OUT[o + 3] / gs / 18.0F, OUT[o + 3] / gs / 18.0F);
                graphics.blitSprite(RenderPipelines.GUI_TEXTURED, Hud.getMobEffectSprite(EFFECTS.get(k).getEffect()), 0, 0, 18, 18, ARGB.white(OUT[o + 4]));
                pose.popMatrix();
            }
        }
        int b = (int) OUT[o++];
        o += b * 4;
        int glyphs = (int) OUT[o++];
        int glyphsAt = o;
        o += glyphs * 7;
        int decor = (int) OUT[o++];
        for (int i = 0; i < decor; i++, o += 5) {
            int kind = (int) OUT[o];
            int x0 = Math.round(OUT[o + 1] / gs), y0 = Math.round(OUT[o + 2] / gs), x1 = Math.round(OUT[o + 3] / gs), y1 = Math.round(OUT[o + 4] / gs);
            if (kind == FIELD && screen instanceof AnvilScreen as) {
                // the game's field where ours is: its clicks land in ours
                var name = ((cursed.recode.mixin.client.PackScreenAccess.Anvil) as).cursed$name();
                if (name != null) {
                    name.setRectangle(Math.max(1, x1 - x0), Math.max(1, y1 - y0), x0, y0);
                }
            } else if (kind == BOOK && screen instanceof EnchantmentScreen es) {
                // the game's book, at its own size, in the middle of its place
                ((cursed.recode.mixin.client.PackScreenAccess.Enchant) es).cursed$book(graphics, Math.round((x0 + x1) / 2.0F - 33.0F), Math.round((y0 + y1) / 2.0F - 29.5F));
            } else if (kind == BANNER && screen instanceof LoomScreen ls && x1 > x0 && y1 > y0) {
                var la = (cursed.recode.mixin.client.PackScreenAccess.Loom) ls;
                var layers = la.cursed$result();
                ItemStack res = screen.getMenu().getSlot(3).getItem();
                if (layers != null && res.getItem() instanceof net.minecraft.world.item.BannerItem bi) {
                    graphics.bannerPattern(la.cursed$flag(), bi.getColor(), layers, x0, y0, x1, y1);
                }
            } else if (kind == MAP && x1 > x0 && y1 > y0) {
                map(mc, screen, graphics, x0, y0, x1, y1);
            } else if (kind == ENTITY && screen instanceof AbstractMountInventoryScreen<?> ms && x1 > x0 && y1 > y0) {
                var mount = ((cursed.recode.mixin.client.PackScreenAccess.Mount) ms).cursed$mount();
                if (mount != null) {
                    InventoryScreen.extractEntityInInventoryFollowsMouse(graphics, x0, y0, x1, y1, Math.max(1, Math.round((y1 - y0) * 0.33F)), 0.25F, mouseX, mouseY, mount);
                }
            }
        }
        // the container's block in the game's px (for the clicks of the next frames)
        frameOf = screen;
        fLeft = OUT[o];
        fTop = OUT[o + 1];
        fK = Math.max(0.01F, OUT[o + 2] / gs * gs);
        fCx0 = OUT[o + 3];
        fCy0 = OUT[o + 4];
        fX0 = OUT[o + 5];
        fY0 = OUT[o + 6];
        fX1 = OUT[o + 7];
        fY1 = OUT[o + 8];
        o += 9;
        int pics = (int) OUT[o++];
        int picsAt = o;
        o += pics * 5;
        float carriedSize = OUT[o] / gs;
        eatsAll = OUT[o + 1] > 0.5F;
        eatsLeft = OUT[o + 2] > 0.5F;
        eatsRight = OUT[o + 3] > 0.5F;
        panelOpen = OUT[o + 4] > 0.5F;
        panelKeys = OUT[o + 5] > 0.5F;
        int recAt = o + 6;
        // the items where the cells are
        graphics.nextStratum();
        var slots = screen.getMenu().slots;
        for (int i = 0; i < r; i++) {
            int q = 2 + i * 10;
            int idx = (int) OUT[q];
            if (idx < 0 || idx >= shownStacks.size() || OUT[q + 4] < 0.02F) {
                continue;
            }
            ItemStack s = shownStacks.get(idx);
            if (s.isEmpty()) {
                continue;
            }
            HudInv.item(graphics, s, OUT[q + 1] / gs, OUT[q + 2] / gs, OUT[q + 3] / gs, 1.0F, idx < slots.size() ? slots.get(idx).x + slots.get(idx).y * 176 : idx);
        }
        // the extra items (a stonecutter's recipes…)
        for (int i = 0; i < pics; i++) {
            int q = picsAt + i * 5;
            int kind = (int) OUT[q];
            int idx = (int) OUT[q + 1];
            float x = OUT[q + 2] / gs, y = OUT[q + 3] / gs, size = OUT[q + 4] / gs;
            if (kind == 0 && idx >= 0 && idx < EXTRA.size() && !EXTRA.get(idx).isEmpty()) {
                HudInv.item(graphics, EXTRA.get(idx), x, y, size, 1.0F, 1000 + idx);
            } else if (kind == 1 && idx >= 0 && idx < PATTERNS.size()) {
                // the loom's pattern as its button shows it: on grey, the pattern's own picture
                var sprite = graphics.getSprite(net.minecraft.client.renderer.Sheets.getBannerSprite(PATTERNS.get(idx)));
                var pose = graphics.pose();
                pose.pushMatrix();
                pose.translate(x, y);
                pose.scale(size / 14.0F, size / 14.0F);
                pose.translate(4.5F, 2.0F);
                float u0 = sprite.getU0(), u1 = u0 + (sprite.getU1() - sprite.getU0()) * 21.0F / 64.0F;
                float vh = sprite.getV1() - sprite.getV0(), v0 = sprite.getV0() + vh / 64.0F, v1 = v0 + vh * 40.0F / 64.0F;
                graphics.fill(0, 0, 5, 10, net.minecraft.world.item.DyeColor.GRAY.getTextureDiffuseColor());
                graphics.blit(sprite.atlasLocation(), 0, 0, 5, 10, u0, u1, v0, v1);
                pose.popMatrix();
            } else if (kind == 2 && idx >= 0 && idx < BEACON.size()) {
                var pose = graphics.pose();
                pose.pushMatrix();
                pose.translate(x, y);
                pose.scale(size / 18.0F, size / 18.0F);
                graphics.blitSprite(RenderPipelines.GUI_TEXTURED, Hud.getMobEffectSprite(BEACON.get(idx)), 0, 0, 18, 18);
                pose.popMatrix();
            }
        }
        // the recipe book's tabs and recipes
        HudRecipes.draw(graphics, OUT, recAt, gs);
        // the carried stack, then the picture over all
        ItemStack carried = screen.getMenu().getCarried();
        graphics.nextStratum();
        if (!carried.isEmpty()) {
            HudInv.item(graphics, carried, rawX - carriedSize / 2.0F, rawY - carriedSize / 2.0F, carriedSize, 1.06F, 0);
        }
        graphics.nextStratum();
        HudInv.over(graphics);
        // a pack's glyphs in the tooltip and the title
        for (int i = 0; i < glyphs; i++) {
            int q = glyphsAt + i * 7;
            HudText.icon(graphics, (int) OUT[q + 1], OUT[q + 2] / gs, OUT[q + 3] / gs, OUT[q + 4] / gs, OUT[q + 5] / gs, OUT[q + 6]);
        }
        // an item whose tooltip has a picture of its own (a bundle, a map): the game's tooltip
        Slot hov = acc.cursed$hovered();
        if (hov != null && hov.hasItem() && carried.isEmpty() && hov.getItem().getTooltipImage().isPresent()) {
            graphics.setTooltipForNextFrame(mc.font, hov.getItem(), Math.round(rawX), Math.round(rawY));
        }
    }

    // ------------------------------------------------------------------------------------------ what the game knows

    /**
     * The frame's numbers (native PackF::parse): gs, since open, the pointer (window px), the hotbar's chosen slot, the
     * carried count, the kind (0 the survival inventory, 1 a container), the slots (11 each), the effects (3 each), the
     * tooltip's lines; their words in `text`. `shown` gets each slot's stack as drawn (a drag's spread too).
     */
    private static double[] facts(Minecraft mc, LocalPlayer p, AbstractContainerScreen<?> screen, float partial, StringBuilder text, List<ItemStack> shown, int gameX, int gameY) {
        List<Double> n = new ArrayList<>(2048);
        var menu = screen.getMenu();
        var acc = (ContainerScreenAccess) screen;
        n.add((double) gs);
        n.add((System.nanoTime() - openAt) / 1e9);
        n.add(mc.mouseHandler.xpos());
        n.add(mc.mouseHandler.ypos());
        n.add((double) p.getInventory().getSelectedSlot());
        ItemStack carried = menu.getCarried();
        n.add((double) carried.getCount());
        n.add(screen instanceof InventoryScreen ? 0.0 : 1.0);
        Component title = screen instanceof InventoryScreen ? Component.empty() : screen.getTitle();
        if (screen instanceof MerchantScreen && menu instanceof net.minecraft.world.inventory.MerchantMenu mm && mm.showProgressBar() && mm.getTraderLevel() > 0 && mm.getTraderLevel() <= 5) {
            title = Component.translatable("merchant.title", screen.getTitle(), Component.translatable("merchant.level." + mm.getTraderLevel()));
        }
        text.append(HudText.encode(90, title)).append('\n');
        var slots = menu.slots;
        boolean quick = acc.cursed$quick();
        var quickSlots = acc.cursed$quickSlots();
        // the recipe the server could not fill: what is missing, shown faint in its slots
        var ghosts = HudRecipes.ghosts(screen);
        n.add((double) slots.size());
        for (Slot sl : slots) {
            int[] place = role(menu, sl, p);
            ItemStack s = sl.getItem();
            boolean ghost = false;
            if (s.isEmpty() && !ghosts.isEmpty()) {
                ItemStack g = ghosts.get(sl);
                if (g != null && !g.isEmpty()) {
                    s = g;
                    ghost = true;
                }
            }
            int preview = -1;
            boolean drag = quick && quickSlots.contains(sl);
            // a stack spread over the slots by dragging: what each gets
            if (drag && !carried.isEmpty() && quickSlots.size() > 1 && AbstractContainerMenu.canItemQuickReplace(sl, carried, true) && menu.canDragTo(sl)) {
                int most = Math.min(carried.getMaxStackSize(), sl.getMaxStackSize(carried));
                int had = s.isEmpty() ? 0 : s.getCount();
                preview = Math.min(AbstractContainerMenu.getQuickCraftPlaceCount(quickSlots.size(), acc.cursed$quickType(), carried) + had, most);
                s = carried.copyWithCount(preview);
            }
            shown.add(s);
            n.add((double) place[0]);
            n.add((double) place[1]);
            n.add((double) place[2]);
            n.add((double) s.getCount());
            n.add(s.isDamageableItem() ? (double) s.getDamageValue() : 0.0);
            n.add(s.isDamageableItem() ? (double) s.getMaxDamage() : 0.0);
            n.add((double) preview);
            n.add(drag ? 1.0 : 0.0);
            n.add(s.isEmpty() ? 0.0 : 1.0);
            n.add(s.isEmpty() ? 0.0 : (double) p.getCooldowns().getCooldownPercent(s, partial));
            n.add((menu instanceof CrafterMenu cm && sl.index < 9 && cm.isSlotDisabled(sl.index) ? 1.0 : 0.0) + (ghost ? 2.0 : 0.0));
        }
        // the effects, the longest first
        EFFECTS.clear();
        EFFECTS.addAll(p.getActiveEffects());
        EFFECTS.sort((a, b) -> Integer.compare(b.getDuration(), a.getDuration()));
        if (EFFECTS.size() > 8) {
            EFFECTS.subList(8, EFFECTS.size()).clear();
        }
        n.add((double) EFFECTS.size());
        for (MobEffectInstance e : EFFECTS) {
            int d = e.isInfiniteDuration() ? -1 : e.getDuration();
            int first = FIRST.merge(e.getEffect(), Math.max(d, 1), Math::max);
            n.add((double) d);
            n.add((double) e.getAmplifier());
            n.add((double) first);
            text.append(HudInv.clean(e.getEffect().value().getDisplayName().getString())).append('\n');
        }
        // the decorations (their own tooltip, if one is pointed at), then the tooltip
        EXTRA.clear();
        ownTip = null;
        List<Double> deco = new ArrayList<>();
        StringBuilder decoText = new StringBuilder();
        decorations(mc, p, screen, deco, decoText, gameX, gameY);
        Slot hov = acc.cursed$hovered();
        if (ownTip == null && carried.isEmpty()) {
            ownTip = HudRecipes.tip(mc);
        }
        // a missing ingredient pointed at: what it is
        if (ownTip == null && carried.isEmpty() && hov != null && !hov.hasItem() && ghosts.containsKey(hov)) {
            ownTip = Screen.getTooltipFromItem(mc, ghosts.get(hov));
        }
        if (ownTip != null && carried.isEmpty()) {
            int k = Math.min(ownTip.size(), 40);
            n.add((double) k);
            for (int i = 0; i < k; i++) {
                text.append(HudText.encode(100 + i, ownTip.get(i))).append('\n');
            }
        } else if (hov != null && hov.hasItem() && carried.isEmpty() && hov.getItem().getTooltipImage().isEmpty()) {
            List<Component> lines = Screen.getTooltipFromItem(mc, hov.getItem());
            int k = Math.min(lines.size(), 40);
            n.add((double) k);
            for (int i = 0; i < k; i++) {
                text.append(HudText.encode(100 + i, lines.get(i))).append('\n');
            }
        } else {
            n.add(0.0);
        }
        n.addAll(deco);
        text.append(decoText);
        n.add((double) EXTRA.size());
        for (ItemStack s : EXTRA) {
            n.add((double) s.getCount());
        }
        // the mouse's buttons (the blocks' editor drags with them), what is asked of the editor
        n.add((double) (held | pressed));
        pressed = 0;
        n.add((double) ask);
        ask = 0;
        HudRecipes.facts(mc, screen, n, text);
        double[] out = new double[n.size()];
        for (int i = 0; i < out.length; i++) {
            out[i] = n.get(i);
        }
        return out;
    }

    /**
     * What the container shows besides its slots, where the game draws it (its px from the screen's corner): 7 numbers
     * each (kind, x0, y0, x1, y1, v, w), a field's and a label's words in `text`.
     */
    private static void decorations(Minecraft mc, LocalPlayer p, AbstractContainerScreen<?> screen, List<Double> n, StringBuilder text, int gameX, int gameY) {
        List<double[]> d = new ArrayList<>();
        List<String> words = new ArrayList<>();
        var menu = screen.getMenu();
        if (menu instanceof AbstractFurnaceMenu fm) {
            d.add(new double[] {FIRE, 56, 36, 70, 50, fm.isLit() ? fm.getLitProgress() : 0.0, 0});
            d.add(new double[] {ARROW, 0, 0, 0, 0, fm.getBurnProgress(), 0});
        } else if (menu instanceof BrewingStandMenu bm) {
            int total = bm.getTotalFuel();
            d.add(new double[] {FUEL, 60, 43, 78, 47, total > 0 ? Math.min(1.0, (double) bm.getFuel() / total) : 0.0, 0});
            int t = bm.getBrewingTicks(), all = bm.getTotalBrewingTicks();
            d.add(new double[] {BREW, 97, 16, 106, 44, t > 0 && all > 0 ? 1.0 - (double) t / all : 0.0, 0});
            d.add(new double[] {BUBBLES, 63, 14, 75, 43, t > 0 ? (t / 2 % 14) / 14.0 + 0.001 : 0.0, 0});
        } else if (screen instanceof CartographyTableScreen) {
            // the map as it will come out (Java draws it in its well), a cross when it can't
            d.add(new double[] {MAP, 67, 13, 133, 79, 0, 0});
            ItemStack add = menu.getSlot(1).getItem();
            var id = menu.getSlot(0).getItem().get(net.minecraft.core.component.DataComponents.MAP_ID);
            var data = id != null ? net.minecraft.world.item.MapItem.getSavedData(id, mc.level) : null;
            boolean paper = add.is(net.minecraft.world.item.Items.PAPER), pane = add.is(net.minecraft.world.item.Items.GLASS_PANE);
            if (data != null && (data.locked && (paper || pane)
                    || paper && (!menu.getSlot(0).getItem().is(net.minecraft.tags.ItemTags.EXTENDABLE_MAPS) || data.scale >= 4))) {
                d.add(new double[] {ERROR, 35, 31, 63, 52, 0, 0});
            }
        } else if (screen instanceof CraftingScreen || screen instanceof GrindstoneScreen) {
            d.add(new double[] {ARROW, 0, 0, 0, 0, -1, 0});
            if (screen instanceof GrindstoneScreen && (menu.getSlot(0).hasItem() || menu.getSlot(1).hasItem()) && !menu.getSlot(2).hasItem()) {
                d.add(new double[] {ERROR, 0, 0, 0, 0, 0, 0});
            }
        } else if (menu instanceof SmithingMenu sm) {
            d.add(new double[] {ARROW, 0, 0, 0, 0, -1, 0});
            if (sm.hasRecipeError()) {
                d.add(new double[] {ERROR, 0, 0, 0, 0, 0, 0});
            }
        } else if (menu instanceof AnvilMenu am && screen instanceof AnvilScreen as) {
            d.add(new double[] {ARROW, 0, 0, 0, 0, -1, 0});
            d.add(new double[] {PLUS, 51, 47, 67, 63, 0, 0});
            var name = ((cursed.recode.mixin.client.PackScreenAccess.Anvil) as).cursed$name();
            if (name != null) {
                var ea = (cursed.recode.mixin.client.EditBoxAccess) name;
                d.add(new double[] {FIELD, 62, 24, 165, 36, name.isFocused() ? name.getCursorPosition() : -1, ea.cursed$highlight()});
                words.add(HudText.encode(139 + d.size(), Component.literal(name.getValue())));
            }
            if ((menu.getSlot(0).hasItem() || menu.getSlot(1).hasItem()) && !menu.getSlot(am.getResultSlot()).hasItem()) {
                d.add(new double[] {ERROR, 0, 0, 0, 0, 0, 0});
            }
            int cost = am.getCost();
            if (cost > 0) {
                Component c = null;
                int tone = 1;
                if (cost >= 40 && !p.hasInfiniteMaterials()) {
                    c = Component.translatable("container.repair.expensive");
                    tone = 2;
                } else if (menu.getSlot(2).hasItem()) {
                    c = Component.translatable("container.repair.cost", cost);
                    tone = menu.getSlot(2).mayPickup(p) ? 1 : 2;
                }
                if (c != null) {
                    d.add(new double[] {LABEL, 60, 66, 168, 78, tone, 0});
                    words.add(HudText.encode(139 + d.size(), Component.literal(c.getString())));
                }
            }
        } else if (menu instanceof CrafterMenu cm) {
            d.add(new double[] {ARROW, 0, 0, 0, 0, -1, 0});
            d.add(new double[] {POWER, 97, 52, 113, 66, cm.isPowered() ? 1.0 : 0.0, 0});
        } else if (screen instanceof AbstractMountInventoryScreen<?>) {
            d.add(new double[] {ENTITY, 26, 18, 78, 70, 0, 0});
        } else if (menu instanceof EnchantmentMenu em) {
            d.add(new double[] {BOOK, 14, 14, 52, 45, 0, 0});
            int lapis = em.getGoldCount();
            var reg = mc.level.registryAccess().lookupOrThrow(net.minecraft.core.registries.Registries.ENCHANTMENT);
            int left = ((ContainerScreenAccess) screen).cursed$left(), top = ((ContainerScreenAccess) screen).cursed$top();
            for (int i = 0; i < 3; i++) {
                int cost = em.costs[i];
                boolean can = cost > 0 && (p.hasInfiniteMaterials() || lapis >= i + 1 && p.experienceLevel >= cost);
                var clue = reg.get(em.enchantClue[i]);
                int level = em.levelClue[i];
                Component name = clue.isPresent() && level >= 0
                        ? Component.translatable("container.enchant.clue", net.minecraft.world.item.enchantment.Enchantment.getFullname(clue.get(), level))
                        : Component.empty();
                d.add(new double[] {OPTION, 60, 14 + 19 * i, 168, 33 + 19 * i, cost, can ? 1 : 0, i});
                words.add(HudText.encode(139 + d.size(), name));
                // the game's tooltip of it, pointed at
                int dx = gameX - (left + 60), dy = gameY - (top + 14 + 19 * i);
                if (cost > 0 && clue.isPresent() && level >= 0 && dx >= 0 && dy >= 0 && dx < 108 && dy < 17) {
                    List<Component> tip = new ArrayList<>();
                    tip.add(name.copy().withStyle(net.minecraft.ChatFormatting.WHITE));
                    if (!p.hasInfiniteMaterials()) {
                        tip.add(Component.empty());
                        if (p.experienceLevel < cost) {
                            tip.add(Component.translatable("container.enchant.level.requirement", cost).withStyle(net.minecraft.ChatFormatting.RED));
                        } else {
                            int need = i + 1;
                            tip.add((need == 1 ? Component.translatable("container.enchant.lapis.one") : Component.translatable("container.enchant.lapis.many", need))
                                    .withStyle(lapis >= need ? net.minecraft.ChatFormatting.GRAY : net.minecraft.ChatFormatting.RED));
                            tip.add((need == 1 ? Component.translatable("container.enchant.level.one") : Component.translatable("container.enchant.level.many", need))
                                    .withStyle(net.minecraft.ChatFormatting.GRAY));
                        }
                    }
                    ownTip = tip;
                }
            }
        } else if (menu instanceof StonecutterMenu sm && screen instanceof StonecutterScreen ss) {
            var acc = (cursed.recode.mixin.client.PackScreenAccess.Cutter) ss;
            int start = acc.cursed$start();
            var set = sm.getVisibleRecipes();
            int total = sm.hasInputItem() ? set.size() : 0;
            int shown = Math.max(0, Math.min(12, total - start));
            int first = EXTRA.size();
            var ctx = net.minecraft.world.item.crafting.display.SlotDisplayContext.fromLevel(mc.level);
            int left = ((ContainerScreenAccess) screen).cursed$left(), top = ((ContainerScreenAccess) screen).cursed$top();
            for (int i = 0; i < shown; i++) {
                ItemStack out = set.entries().get(start + i).recipe().optionDisplay().resolveForFirstStack(ctx);
                EXTRA.add(out);
                int cx = left + 52 + i % 4 * 16, cy = top + 14 + i / 4 * 18;
                if (gameX >= cx && gameX < cx + 16 && gameY >= cy && gameY < cy + 18) {
                    ownTip = Screen.getTooltipFromItem(mc, out);
                }
            }
            int chosen = sm.getSelectedRecipeIndex() - start;
            d.add(new double[] {GRID, 52, 14, 116, 68, first, shown, chosen >= 0 && chosen < shown ? chosen : -1});
            d.add(new double[] {SCROLL, 119, 15, 131, 70, acc.cursed$scroll(), total > 12 ? 1 : 0});
        } else if (menu instanceof net.minecraft.world.inventory.MerchantMenu mm && screen instanceof MerchantScreen ms) {
            merchant(mc, mm, ms, d, gameX, gameY);
        } else if (menu instanceof net.minecraft.world.inventory.LoomMenu lm && screen instanceof LoomScreen ls) {
            var la = (cursed.recode.mixin.client.PackScreenAccess.Loom) ls;
            PATTERNS.clear();
            if (la.cursed$display()) {
                PATTERNS.addAll(lm.getSelectablePatterns());
            }
            int first = la.cursed$startRow() * 4;
            int shown = Math.max(0, Math.min(16, PATTERNS.size() - first));
            int chosen = lm.getSelectedBannerPatternIndex() - first;
            d.add(new double[] {LOOM, 60, 13, 116, 69, first, shown, chosen >= 0 && chosen < shown ? chosen : -1});
            d.add(new double[] {SCROLL, 119, 13, 131, 69, la.cursed$scroll(), la.cursed$display() && PATTERNS.size() > 16 ? 1 : 0});
            d.add(new double[] {BANNER, 141, 8, 161, 48, 0, 0});
            if (la.cursed$max()) {
                d.add(new double[] {ERROR, 138, 52, 164, 78, 0, 0});
            }
            int left = ((ContainerScreenAccess) screen).cursed$left(), top = ((ContainerScreenAccess) screen).cursed$top();
            for (int i = 0; i < shown; i++) {
                int cx = left + 60 + i % 4 * 14, cy = top + 13 + i / 4 * 14;
                if (gameX >= cx && gameX < cx + 14 && gameY >= cy && gameY < cy + 14) {
                    var dye = la.cursed$dye().getOrDefault(net.minecraft.core.component.DataComponents.DYE, net.minecraft.world.item.DyeColor.WHITE);
                    ownTip = List.of(Component.translatable(PATTERNS.get(first + i).value().translationKey() + "." + dye.getName()));
                }
            }
        } else if (screen instanceof BeaconScreen bs) {
            beacon(mc, bs, d, words, gameX, gameY);
        }
        n.add((double) d.size());
        for (double[] q : d) {
            for (int k = 0; k < 8; k++) {
                n.add(k < q.length ? q[k] : 0.0);
            }
        }
        for (String w : words) {
            text.append(w).append('\n');
        }
    }

    /** A slot's place in the layout: its role, then column and row (armour: which piece; a container's: the game's x, y). */
    private static int[] role(AbstractContainerMenu menu, Slot s, LocalPlayer p) {
        if (!s.isActive()) {
            return new int[] {HIDDEN, 0, 0};
        }
        if (s.container == p.getInventory()) {
            int i = s.getContainerSlot();
            if (i < 9) {
                return new int[] {HOTBAR, i, 0};
            }
            if (i < 36) {
                return new int[] {BACKPACK, (i - 9) % 9, (i - 9) / 9};
            }
            if (i < 40) {
                return new int[] {ARMOUR, 39 - i, 0};
            }
            return i == 40 ? new int[] {OFFHAND, 0, 0} : new int[] {HIDDEN, 0, 0};
        }
        if (menu instanceof InventoryMenu) {
            if (s instanceof ResultSlot) {
                return new int[] {CRAFTED, 0, 0};
            }
            if (s.container instanceof CraftingContainer) {
                int k = s.getContainerSlot();
                return new int[] {CRAFT, k % 2, k / 2};
            }
        }
        return new int[] {output(menu, s) ? OUTPUT : CONTAINER, s.x, s.y};
    }

    private static final net.minecraft.client.renderer.state.MapRenderState MAP_STATE = new net.minecraft.client.renderer.state.MapRenderState();

    /** The cartography table's map in its well (GUI px), as the game's screen shows it. */
    private static void map(Minecraft mc, AbstractContainerScreen<?> screen, GuiGraphicsExtractor graphics, int x0, int y0, int x1, int y1) {
        var menu = screen.getMenu();
        var id = menu.getSlot(0).getItem().get(net.minecraft.core.component.DataComponents.MAP_ID);
        if (id == null || mc.level == null) {
            return;
        }
        var data = net.minecraft.world.item.MapItem.getSavedData(id, mc.level);
        if (data == null) {
            return;
        }
        float side = Math.min(x1 - x0, y1 - y0) * 0.86F;
        var pose = graphics.pose();
        pose.pushMatrix();
        pose.translate((x0 + x1) / 2.0F - side / 2.0F, (y0 + y1) / 2.0F - side / 2.0F);
        pose.scale(side / 128.0F, side / 128.0F);
        mc.getMapRenderer().extractRenderState(id, data, MAP_STATE);
        graphics.map(MAP_STATE);
        pose.popMatrix();
    }

    /** A trader's offers (seven at a time, as its buttons), its scroll, its experience; its buttons seen (they take the
     *  clicks only seen: the game sets that as it draws them). */
    private static void merchant(Minecraft mc, net.minecraft.world.inventory.MerchantMenu mm, MerchantScreen ms, List<double[]> d, int gameX, int gameY) {
        var acc = (cursed.recode.mixin.client.PackScreenAccess.Trader) ms;
        var offers = mm.getOffers();
        d.add(new double[] {ARROW, 0, 0, 0, 0, -1, 0});
        int scroll = acc.cursed$scroll();
        int chosen = acc.cursed$chosen();
        int b = 0;
        for (var w : ms.children()) {
            if (w instanceof net.minecraft.client.gui.components.AbstractWidget aw && aw.getClass().getSimpleName().contains("TradeOffer")) {
                aw.visible = b < offers.size();
                b++;
            }
        }
        int left = ((ContainerScreenAccess) ms).cursed$left(), top = ((ContainerScreenAccess) ms).cursed$top();
        for (int i = 0; i < 7 && scroll + i < offers.size(); i++) {
            var offer = offers.get(scroll + i);
            int base = EXTRA.size();
            ItemStack a = offer.getCostA(), b2 = offer.getCostB(), r = offer.getResult();
            EXTRA.add(a);
            EXTRA.add(b2);
            EXTRA.add(r);
            boolean cheaper = offer.getBaseCostA().getCount() != a.getCount();
            int flags = (offer.isOutOfStock() ? 1 : 0) | (scroll + i == chosen ? 2 : 0) | (cheaper ? 4 : 0);
            d.add(new double[] {TRADE, 5, 18 + 20 * i, 93, 38 + 20 * i, base, flags, cheaper ? offer.getBaseCostA().getCount() : 0});
            // the game's tooltip of what is pointed at in it
            int bx = left + 5, by = top + 18 + 20 * i;
            if (gameY >= by && gameY < by + 20 && gameX >= bx && gameX < bx + 88) {
                ItemStack tip = gameX < bx + 20 ? a : gameX > bx + 30 && gameX < bx + 50 ? b2 : gameX > bx + 65 ? r : ItemStack.EMPTY;
                if (!tip.isEmpty()) {
                    ownTip = Screen.getTooltipFromItem(mc, tip);
                }
            }
        }
        int over = Math.max(1, offers.size() - 7);
        d.add(new double[] {SCROLL, 94, 18, 100, 157, offers.size() > 7 ? (double) scroll / over : 0.0, offers.size() > 7 ? 1 : 0});
        int level = mm.getTraderLevel();
        if (mm.showProgressBar() && level > 0 && level < 5) {
            int min = net.minecraft.world.entity.npc.villager.VillagerData.getMinXpPerLevel(level);
            int max = net.minecraft.world.entity.npc.villager.VillagerData.getMaxXpPerLevel(level);
            int xp = mm.getTraderXp();
            double now = max > min ? Math.max(0.0, (double) (xp - min) / (max - min)) : 0.0;
            double then = max > min ? Math.max(0.0, (double) (xp - min + mm.getFutureTraderXp()) / (max - min)) : 0.0;
            d.add(new double[] {XPBAR, 136, 16, 238, 21, now, then});
        }
        if (chosen >= 0 && chosen < offers.size() && offers.get(chosen).isOutOfStock()) {
            d.add(new double[] {ERROR, 186, 35, 208, 56, 0, 0});
        }
    }

    /** A beacon's buttons as the game has them (its powers, done, cancel), its words, the items it takes. */
    private static void beacon(Minecraft mc, BeaconScreen bs, List<double[]> d, List<String> words, int gameX, int gameY) {
        BEACON.clear();
        int left = ((ContainerScreenAccess) bs).cursed$left(), top = ((ContainerScreenAccess) bs).cursed$top();
        var acc = (cursed.recode.mixin.client.PackScreenAccess.Beacon) bs;
        for (var w : bs.children()) {
            if (!(w instanceof net.minecraft.client.gui.components.AbstractWidget aw) || !aw.visible) {
                continue;
            }
            String kind = aw.getClass().getSimpleName();
            int flags = aw.active ? 1 : 0;
            int pic = -1;
            Component tip = null;
            if (kind.contains("Confirm")) {
                flags |= 4;
                tip = net.minecraft.network.chat.CommonComponents.GUI_DONE;
            } else if (kind.contains("Cancel")) {
                flags |= 8;
                tip = net.minecraft.network.chat.CommonComponents.GUI_CANCEL;
            } else {
                var effect = beaconEffect(aw);
                if (effect == null) {
                    continue;
                }
                pic = BEACON.size();
                BEACON.add(effect);
                boolean primary = beaconPrimary(aw);
                var chosen = primary ? acc.cursed$primary() : acc.cursed$secondary();
                if (effect.equals(chosen)) {
                    flags |= 2;
                }
                tip = kind.contains("Upgrade") ? Component.translatable("potion.withAmplifier", effect.value().getDisplayName(), Component.translatable("potion.potency.1"))
                        : effect.value().getDisplayName();
            }
            int x = aw.getX() - left, y = aw.getY() - top;
            d.add(new double[] {WIDGET, x, y, x + aw.getWidth(), y + aw.getHeight(), pic, flags});
            if (tip != null && gameX >= aw.getX() && gameX < aw.getX() + aw.getWidth() && gameY >= aw.getY() && gameY < aw.getY() + aw.getHeight()) {
                ownTip = List.of(tip);
            }
        }
        d.add(new double[] {LABEL, 10, 5, 114, 16, 0, 0, 1});
        words.add(HudText.encode(139 + d.size(), Component.translatable("block.minecraft.beacon.primary")));
        d.add(new double[] {LABEL, 117, 5, 221, 16, 0, 0, 1});
        words.add(HudText.encode(139 + d.size(), Component.translatable("block.minecraft.beacon.secondary")));
        int[] xs = {20, 41, 63, 86, 108};
        net.minecraft.world.item.Item[] items = {net.minecraft.world.item.Items.NETHERITE_INGOT, net.minecraft.world.item.Items.EMERALD,
                net.minecraft.world.item.Items.DIAMOND, net.minecraft.world.item.Items.GOLD_INGOT, net.minecraft.world.item.Items.IRON_INGOT};
        for (int i = 0; i < 5; i++) {
            d.add(new double[] {ITEM, xs[i], 109, xs[i] + 16, 125, EXTRA.size(), 0});
            EXTRA.add(new ItemStack(items[i]));
        }
    }

    private static java.lang.reflect.@Nullable Field effectField, primaryField;

    /** A beacon's power button's effect (a field of the game's private button). */
    @SuppressWarnings("unchecked")
    private static net.minecraft.core.@Nullable Holder<MobEffect> beaconEffect(Object button) {
        try {
            Class<?> c = button.getClass();
            while (c != null && !c.getSimpleName().equals("BeaconPowerButton")) {
                c = c.getSuperclass();
            }
            if (c == null) {
                return null;
            }
            if (effectField == null) {
                effectField = c.getDeclaredField("effect");
                effectField.setAccessible(true);
                primaryField = c.getDeclaredField("isPrimary");
                primaryField.setAccessible(true);
            }
            return (net.minecraft.core.Holder<MobEffect>) effectField.get(button);
        } catch (ReflectiveOperationException | RuntimeException e) {
            return null;
        }
    }

    private static boolean beaconPrimary(Object button) {
        try {
            return primaryField != null && primaryField.getBoolean(button);
        } catch (ReflectiveOperationException | RuntimeException e) {
            return false;
        }
    }

    /** A container's result slot (the anvil's, the grindstone's… are plain slots of their own: by their place). */
    private static boolean output(AbstractContainerMenu menu, Slot s) {
        if (s instanceof ResultSlot || s.getClass().getSimpleName().contains("Result")) {
            return true;
        }
        if (menu instanceof net.minecraft.world.inventory.ItemCombinerMenu icm) {
            return s.index == icm.getResultSlot();
        }
        if (menu instanceof net.minecraft.world.inventory.GrindstoneMenu || menu instanceof net.minecraft.world.inventory.CartographyTableMenu) {
            return s.index == 2;
        }
        if (menu instanceof net.minecraft.world.inventory.StonecutterMenu) {
            return s.index == 1;
        }
        if (menu instanceof net.minecraft.world.inventory.LoomMenu) {
            return s.index == 3;
        }
        return false;
    }

    // ------------------------------------------------------------------------------------------ the pointer

    /** The slot under (x, y) (GUI units) in the layout of the last frame. */
    public static @Nullable Slot slotAt(AbstractContainerScreen<?> screen, double x, double y) {
        if (eatsAll && screen == frameOf) {
            return null;
        }
        Minecraft mc = Minecraft.getInstance();
        float wx = (float) mc.mouseHandler.xpos(), wy = (float) mc.mouseHandler.ypos();
        var slots = screen.getMenu().slots;
        int r = outN >= 2 ? (int) OUT[1] : 0;
        for (int i = 0; i < r; i++) {
            int o = 2 + i * 10;
            if (wx >= OUT[o + 5] && wx < OUT[o + 7] && wy >= OUT[o + 6] && wy < OUT[o + 8]) {
                int idx = (int) OUT[o];
                return idx >= 0 && idx < slots.size() && slots.get(idx).isActive() ? slots.get(idx) : null;
            }
        }
        return null;
    }

    /** Whether (x, y) (GUI units) is outside every block (a stack let go there is thrown). */
    public static boolean outside(double x, double y) {
        if (outN < 2) {
            return false;
        }
        Minecraft mc = Minecraft.getInstance();
        float wx = (float) mc.mouseHandler.xpos(), wy = (float) mc.mouseHandler.ypos();
        int o = 2 + (int) OUT[1] * 10 + 5;
        o += 1 + (int) OUT[o] * 5;
        int b = (int) OUT[o++];
        for (int i = 0; i < b; i++, o += 4) {
            if (wx >= OUT[o] && wx < OUT[o + 2] && wy >= OUT[o + 1] && wy < OUT[o + 3]) {
                return false;
            }
        }
        return true;
    }
}
