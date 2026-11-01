package cursed.recode.hud;

import com.google.gson.JsonParser;
import com.mojang.serialization.JsonOps;
import cursed.recode.mixin.client.ContainerScreenAccess;
import cursed.recode.mixin.client.CreativeScreenAccess;
import cursed.recode.render.GuiFrame;
import cursed.recode.render.Native;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.EditBox;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.inventory.AbstractContainerScreen;
import net.minecraft.client.gui.screens.inventory.CreativeModeInventoryScreen;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.core.NonNullList;
import net.minecraft.core.component.DataComponents;
import net.minecraft.world.inventory.ContainerInput;
import net.minecraft.world.inventory.Slot;
import net.minecraft.world.item.CreativeModeTab;
import net.minecraft.world.item.CreativeModeTabs;
import net.minecraft.world.item.ItemStack;
import org.jspecify.annotations.Nullable;

/**
 * Custom Inventory in creative (Interface; native gui/hud/hudcreative.rs): the creative inventory as the client's. The
 * game's screen and menu stay what they are (its tabs, its search, its slots and clicks, the packets) but it is not
 * drawn: this sends the native what the game knows, puts its picture under everything, the items where it laid them
 * out (the grid's list scrolled smoothly, the hotbar, the survival inventory's tab), the inspected item turning, the
 * player on that tab, then its picture over the items. The rail chooses the game's tabs (or the client's own lists:
 * favourites, the recently taken). A grid item under the pointer is the picker's first slot with that item put in it,
 * so the game's clicks and keys work on it as on its own; a click takes a stack, the right one a single item, Shift
 * puts a stack straight into the hotbar, Alt or the middle button makes it a favourite. Anything going wrong: the
 * game's own screen next frame.
 */
public final class HudCreative {
    private HudCreative() {
    }

    /** What the native's `out` names things by (hudcreative.rs): a menu slot by its index, then these. */
    private static final int BIN = 9000, GRID = 10000, RAIL = 20000, SEARCH = 30000, SAVE = 30001, UNDO = 30002, TOGGLE = 30003;
    /** A set's item (KIT + its place × 64 + the player's menu's slot), its buttons (+ its place), a new one. */
    private static final int KIT = 40000, KIT_TAKE = 200000, KIT_OVER = 300000, KIT_DEL = 400000, KIT_NEW = 500000, KIT_NAME = 600000;
    /** How long a button that changes a set stays armed for its second click. */
    private static final long ARMED_NS = 3_000_000_000L;
    /** How long the bin's undo stays after the inventory was emptied. */
    private static final long UNDO_NS = 5_000_000_000L;
    private static final long FLY_NS = 320_000_000L;
    private static final long SWELL_NS = 280_000_000L;

    private static boolean on;
    private static boolean broken;
    private static @Nullable Screen shown;
    private static long openAt;
    private static final float[] OUT = new float[16384];
    private static int outN;
    private static float gs = 1.0F;
    /** The grid's own list (4 favourites, 5 the recently taken) over the game's tab it was chosen on; 0: the tab's. */
    private static int own;
    private static @Nullable CreativeModeTab ownOn;
    private static final List<ItemStack> FAV = new ArrayList<>();
    private static final List<ItemStack> RECENT = new ArrayList<>();
    private static boolean loaded;
    private static double wheel;
    private static int want0;
    private static int want1 = 12 * 12;
    private static long savedAt;
    private static int savedSlot;
    private static long binAt;
    /** The inventory as it was before Shift on the bin emptied it (the player's menu's slots), and when: its undo. */
    private static ItemStack @Nullable [] cleared;
    private static long clearedAt;
    /** The item last pointed at (kept a moment over the gaps between the wells), the inspected one. */
    private static ItemStack pointed = ItemStack.EMPTY;
    private static long pointedAt;
    private static ItemStack inspected = ItemStack.EMPTY;
    private static final NonNullList<ItemStack> BOXED = NonNullList.withSize(27, ItemStack.EMPTY);
    /** The grid item last under the pointer (where a stack taken from it flies from). */
    private static int lastGrid = -1;
    /** A stack flying from the grid to a hotbar slot (Shift, a number key): what, from (x, y, side; window px), to which
     *  hotbar slot, since when; the slot it landed in swells a moment, as a grid item made a favourite. */
    private static ItemStack flyStack = ItemStack.EMPTY;
    private static final float[] FLY_FROM = new float[3];
    private static int flyTo = -1;
    private static long flyAt;
    private static int landed = -1;
    private static long landAt;
    private static int pulsed = -1;
    private static long pulseAt;
    /** The buttons held over the screen (1 the left, 2 the right), pressed since the last frame (4, 8); whose its clicks
     *  are as the last frame said (native hudinvui Eats); a press taken here (its drag and release ours too); what is
     *  asked next frame (2: Esc). */
    private static int held, pressed, ask;
    private static boolean eatsAll, eatsLeft, eatsRight, panelOpen, panelKeys, swallowed;

    /**
     * The sets (her choice, 18.10): the whole inventory as it was — the armour, the backpack, the hotbar, the other
     * hand (the player's menu's slots 5…45) — under a name of one's own (none: "Set N"), as many as one likes, kept in
     * cursed-recode/kits.txt. Put on, a set takes the inventory's place (for a while to be put back); one is written
     * over or deleted with a second click; its name is typed in place; an item taken from one and let go in the bin
     * leaves it. The game's nine saved hotbars (C and X) stay as they are.
     */
    private static final class Kit {
        String name = "";
        final ItemStack[] items = new ItemStack[46];
        /** Its items the game could not read this time (another world's enchantments, an item gone): kept as written,
         *  so a save does not lose them. */
        final java.util.Map<Integer, String> unread = new java.util.HashMap<>();

        Kit() {
            java.util.Arrays.fill(this.items, ItemStack.EMPTY);
        }
    }

    private static final List<Kit> KITS = new ArrayList<>();
    /** A button armed for its second click (which set, 1 write over 2 delete, since when); the set whose name is typed,
     *  the words so far; the set and slot of the item taken from one (-1: none); what the undo puts back (0 emptied, 1 a
     *  set put on). */
    private static int armedKit = -1, armedWhat;
    private static long armedAt;
    private static int renaming = -1;
    private static String renameBuf = "";
    private static int fromKit = -1, fromSlot;
    private static int lastKit = -1;
    private static int clearedKind;

    /** The rail's rows as last sent (what a click on one chooses). */
    private static final List<Row> ROWS = new ArrayList<>();

    /** A row of the rail: 0 a tab of the game, 1 favourites, 2 the recently taken. */
    private record Row(int kind, @Nullable CreativeModeTab tab, int count, boolean sep, boolean on, String name) {
    }

    /** From the HUD's state (Native.hudState: 79, Custom Inventory's). */
    static void state(float[] s) {
        on = s[79] > 0.5F;
    }

    /** The client draws this screen (the creative inventory, the module on, our renderer up). */
    public static boolean active(@Nullable Screen screen) {
        Minecraft mc = Minecraft.getInstance();
        if (!on || broken || !(screen instanceof CreativeModeInventoryScreen) || !GuiFrame.active() || mc.player == null || !mc.player.hasInfiniteMaterials()) {
            return false;
        }
        if (screen != shown) {
            shown = screen;
            openAt = System.nanoTime();
            flyTo = -1;
            landed = -1;
            pulsed = -1;
            pointed = ItemStack.EMPTY;
            wheel = 0.0;
            held = 0;
            pressed = 0;
            eatsAll = eatsLeft = eatsRight = panelOpen = panelKeys = swallowed = false;
            renaming = -1;
            armedKit = -1;
            fromKit = -1;
            want0 = 0;
            want1 = 12 * 12;
            load(mc);
        }
        return true;
    }

    // ------------------------------------------------------------------------------------------ the frame

    /** Instead of the game's background: the frame made, its picture under all, the model, the inspected item. */
    public static void background(CreativeModeInventoryScreen screen, GuiGraphicsExtractor graphics) {
        try {
            frame(screen, graphics);
        } catch (Throwable t) {
            fail(t);
        }
    }

    private static void fail(Throwable t) {
        broken = true;
        org.slf4j.LoggerFactory.getLogger("HudCreative").error("Custom Inventory (creative) failed; the game's own from now on", t);
    }

    private static void frame(CreativeModeInventoryScreen screen, GuiGraphicsExtractor graphics) {
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
        // the game's own search field stays where nothing is (its box would take the clicks on the grid)
        EditBox box = ((CreativeScreenAccess) screen).cursed$search();
        if (box != null) {
            box.setX(-10000);
            box.setY(-10000);
        }
        if (own != 0 && CreativeScreenAccess.cursed$tab() != ownOn) {
            own = 0;
        }
        StringBuilder text = new StringBuilder("creative\n");
        double[] nums = facts(mc, p, screen, text);
        int id = Native.invFrame(w, h, nums, text.toString());
        outN = Native.invOut(OUT);
        int top = Native.invTop();
        if (id == 0 || outN < 2) {
            return;
        }
        HudInv.pictures(graphics, w, h, id, top);
        int tail = oTail();
        want0 = Math.max(0, (int) OUT[tail + 1]);
        want1 = Math.max(want0, (int) OUT[tail + 2]);
        // whose the clicks are (after the blocks: hudinvui Eats)
        int e = tail + 9 + 12;
        eatsAll = OUT[e] > 0.5F;
        eatsLeft = OUT[e + 1] > 0.5F;
        eatsRight = OUT[e + 2] > 0.5F;
        panelOpen = OUT[e + 3] > 0.5F;
        panelKeys = OUT[e + 4] > 0.5F;
        band(graphics);
        // the rail's pictures: the tabs' own items
        int oi = oIcons();
        int n = (int) OUT[oi];
        for (int i = 0; i < n; i++) {
            int o = oi + 1 + i * 5;
            int r = (int) OUT[o];
            if (r >= 0 && r < ROWS.size() && ROWS.get(r).tab() != null && OUT[o + 4] > 0.02F) {
                HudInv.item(graphics, ROWS.get(r).tab().getIconItem(), OUT[o + 1] / gs, OUT[o + 2] / gs, OUT[o + 3] / gs, 1.0F, r);
            }
        }
        int o = oInspect();
        if (OUT[o] > 0.5F && !inspected.isEmpty()) {
            cursed.recode.menus.ItemStage.inspect(graphics, inspected, OUT[o + 1], OUT[o + 2], OUT[o + 3], OUT[o + 4], OUT[o + 5], OUT[o + 6]);
        }
        HudInv.model(mc, p, graphics, OUT, o + 7, gs, ItemStack.EMPTY);
        graphics.disableScissor();
    }

    /** Only the band the screen has opened to (it opens from a line across the middle). */
    private static void band(GuiGraphicsExtractor graphics) {
        int t = oTail();
        graphics.enableScissor(0, (int) Math.floor(OUT[t + 7] / gs), graphics.guiWidth(), (int) Math.ceil(OUT[t + 8] / gs));
    }

    /** Instead of the game's contents: the hovered slot as the layout has it, the items in their places, the flying
     *  stack, the carried one, the picture over all. */
    public static void contents(CreativeModeInventoryScreen screen, GuiGraphicsExtractor graphics, int mouseX, int mouseY) {
        try {
            items(screen, graphics, mouseX, mouseY);
        } catch (Throwable t) {
            fail(t);
        }
    }

    private static void items(CreativeModeInventoryScreen screen, GuiGraphicsExtractor graphics, int mouseX, int mouseY) {
        var acc = (ContainerScreenAccess) screen;
        acc.cursed$setHovered(slotAt(screen, mouseX, mouseY));
        if (outN < 2) {
            return;
        }
        var slots = screen.getMenu().slots;
        List<ItemStack> list = list(screen);
        long now = System.nanoTime();
        boolean flying = flyTo >= 0 && now - flyAt < FLY_NS;
        if (flyTo >= 0 && !flying) {
            landed = flyTo;
            landAt = flyAt + FLY_NS;
            flyTo = -1;
        }
        int hotBase = screen.isInventoryOpen() ? 36 : 45;
        int tail = oTail();
        band(graphics);
        int r = (int) OUT[1];
        for (int i = 0; i < r; i++) {
            int o = 2 + i * 10;
            int code = (int) OUT[o];
            if (OUT[o + 5] < 0.02F) {
                continue;
            }
            boolean grid = code >= GRID && code < RAIL;
            ItemStack s;
            float lift = OUT[o + 4];
            if (grid) {
                int k = code - GRID;
                s = k < list.size() ? list.get(k) : ItemStack.EMPTY;
                if (k == pulsed) {
                    lift *= swell(now, pulseAt);
                }
            } else if (code >= KIT && code < KIT + 64 * KITS.size()) {
                s = kitItem(code);
            } else if (code < BIN && code < slots.size()) {
                s = slots.get(code).getItem();
                int hot = code - hotBase;
                if (hot >= 0 && hot < 9) {
                    if (flying && hot == flyTo) {
                        continue;
                    }
                    if (hot == landed) {
                        lift *= swell(now, landAt);
                    }
                }
            } else {
                continue;
            }
            if (s.isEmpty()) {
                continue;
            }
            if (grid || code >= KIT && code < KIT_TAKE) {
                // the grid's items (the sets' too) only inside its box
                graphics.enableScissor((int) Math.floor(OUT[tail + 3] / gs), (int) Math.floor(OUT[tail + 4] / gs), (int) Math.ceil(OUT[tail + 5] / gs), (int) Math.ceil(OUT[tail + 6] / gs));
            }
            HudInv.item(graphics, s, OUT[o + 1] / gs, OUT[o + 2] / gs, OUT[o + 3] / gs, lift, code);
            if (grid || code >= KIT && code < KIT_TAKE) {
                graphics.disableScissor();
            }
        }
        graphics.disableScissor();
        graphics.nextStratum();
        // the stack flying into the hotbar: out of its well, over an arc, into its slot
        if (flying) {
            float[] to = iconOf(hotBase + flyTo);
            if (to != null) {
                float t = (now - flyAt) / (float) FLY_NS;
                float e = 1.0F - (1.0F - t) * (1.0F - t) * (1.0F - t);
                float x = FLY_FROM[0] + (to[0] - FLY_FROM[0]) * e;
                float y = FLY_FROM[1] + (to[1] - FLY_FROM[1]) * e - (float) Math.sin(Math.PI * t) * 60.0F * gs;
                float side = FLY_FROM[2] + (to[2] - FLY_FROM[2]) * e;
                HudInv.item(graphics, flyStack, x / gs, y / gs, side / gs, 1.0F + 0.15F * (float) Math.sin(Math.PI * t), 0);
            }
        }
        float side = OUT[tail] / gs;
        var carried = screen.getMenu().getCarried();
        if (!carried.isEmpty()) {
            HudInv.item(graphics, carried, mouseX - side / 2.0F, mouseY - side / 2.0F, side, 1.08F, 0);
        }
        graphics.nextStratum();
        HudInv.over(graphics);
        // a box's contents in the tooltip (over its card)
        graphics.nextStratum();
        int ob = oBoxed();
        int k = (int) OUT[ob];
        for (int i = 0; i < k; i++) {
            int o = ob + 1 + i * 4;
            int b = (int) OUT[o];
            if (b >= 0 && b < BOXED.size() && !BOXED.get(b).isEmpty()) {
                HudInv.item(graphics, BOXED.get(b), OUT[o + 1] / gs, OUT[o + 2] / gs, OUT[o + 3] / gs, 1.0F, b);
            }
        }
    }

    /** A moment's swelling (1.2 down to 1) since `at`. */
    private static float swell(long now, long at) {
        float t = (now - at) / (float) SWELL_NS;
        if (t < 0.0F || t >= 1.0F) {
            return 1.0F;
        }
        return 1.0F + 0.2F * (1.0F - t) * (1.0F - t);
    }

    // ------------------------------------------------------------------------------------------ the layout's places

    private static int oBoxes() {
        return 2 + (int) OUT[1] * 10;
    }

    private static int oIcons() {
        int b = oBoxes();
        return b + 1 + (int) OUT[b] * 5;
    }

    private static int oInspect() {
        int i = oIcons();
        return i + 1 + (int) OUT[i] * 5;
    }

    private static int oBoxed() {
        return oInspect() + 7 + 11;
    }

    private static int oTail() {
        int k = oBoxed();
        return k + 1 + (int) OUT[k] * 4;
    }

    /** What is at (x, y) window px in the last frame's layout (-1 nothing). */
    private static int codeAt(float wx, float wy) {
        if (outN < 2) {
            return -1;
        }
        int ob = oBoxes();
        int b = (int) OUT[ob];
        for (int i = 0; i < b; i++) {
            int o = ob + 1 + i * 5;
            if (wx >= OUT[o + 1] && wx < OUT[o + 3] && wy >= OUT[o + 2] && wy < OUT[o + 4]) {
                return (int) OUT[o];
            }
        }
        int r = (int) OUT[1];
        for (int i = 0; i < r; i++) {
            int o = 2 + i * 10;
            if (wx >= OUT[o + 6] && wx < OUT[o + 8] && wy >= OUT[o + 7] && wy < OUT[o + 9]) {
                return (int) OUT[o];
            }
        }
        return -1;
    }

    /** Where the item of `code` is drawn (x, y, side; window px), if it is. */
    private static float @Nullable [] iconOf(int code) {
        int r = (int) OUT[1];
        for (int i = 0; i < r; i++) {
            int o = 2 + i * 10;
            if ((int) OUT[o] == code) {
                return new float[] {OUT[o + 1], OUT[o + 2], OUT[o + 3]};
            }
        }
        return null;
    }

    /** The slot under (x, y) (GUI units) in the last frame's layout: a menu slot; the bin on the survival inventory's
     *  tab (the game's own); a grid item as the picker's first slot with it put in. */
    public static @Nullable Slot slotAt(AbstractContainerScreen<?> screen, double x, double y) {
        if (!(screen instanceof CreativeModeInventoryScreen cs) || eatsAll) {
            return null;
        }
        int code = codeAt((float) (x * gs), (float) (y * gs));
        var menu = cs.getMenu();
        if (code == BIN) {
            return cs.isInventoryOpen() ? ((CreativeScreenAccess) cs).cursed$bin() : null;
        }
        if (code >= KIT && code < KIT + 64 * KITS.size()) {
            ItemStack st = kitItem(code);
            if (st.isEmpty() || menu.slots.isEmpty()) {
                return null;
            }
            lastKit = code;
            lastGrid = -1;
            Slot first = menu.slots.get(0);
            first.set(st.copy());
            return first;
        }
        if (code >= GRID && code < RAIL) {
            lastKit = -1;
            List<ItemStack> list = list(cs);
            int k = code - GRID;
            if (cs.isInventoryOpen() || k >= list.size() || menu.slots.isEmpty()) {
                return null;
            }
            lastGrid = code;
            Slot first = menu.slots.get(0);
            first.set(list.get(k));
            return first;
        }
        if (code >= 0 && code < BIN && code < menu.slots.size()) {
            Slot s = menu.slots.get(code);
            return s.isActive() ? s : null;
        }
        return null;
    }

    /** Whether (x, y) (GUI units) is outside the screen for the game (a stack let go there is thrown): off its blocks
     *  (the rail, the grid's, the hotbar's) and the gap between them. */
    public static boolean outside(double x, double y) {
        if (outN < 2) {
            return false;
        }
        float px = (float) (x * gs), py = (float) (y * gs);
        int b = oTail() + 9;
        // the gap between the blocks counts as theirs
        float g = 14.0F;
        for (int i = 0; i < 3; i++) {
            int o = b + i * 4;
            if (px >= OUT[o] - g && px < OUT[o + 2] + g && py >= OUT[o + 1] - g && py < OUT[o + 3] + g) {
                return false;
            }
        }
        return true;
    }

    // ------------------------------------------------------------------------------------------ the player's doing

    /** A click the client's own layout takes (the rail, the search, the saving, the bin off the survival inventory's
     *  tab, a favourite); the rest goes to the game. */
    public static boolean click(CreativeModeInventoryScreen s, MouseButtonEvent event) {
        int code = codeAt((float) (event.x() * gs), (float) (event.y() * gs));
        int button = HudLayer.button(event.button());
        held |= button == 0 ? 1 : button == 1 ? 2 : 0;
        pressed |= button == 0 ? 4 : button == 1 ? 8 : 0;
        // a name being typed: a click anywhere but on it keeps it
        if (renaming >= 0 && code != KIT_NAME + renaming) {
            endRename(true);
        }
        // a header's, the settings': the native's (next frame)
        if (eatsAll || button == 0 && eatsLeft || button == 1 && eatsRight) {
            swallowed = true;
            return true;
        }
        Minecraft mc = Minecraft.getInstance();
        var acc = (CreativeScreenAccess) s;
        if (code >= RAIL && code < SEARCH) {
            int r = code - RAIL;
            if (button == 0 && r < ROWS.size()) {
                choose(s, ROWS.get(r));
            }
            return true;
        }
        if (code == SEARCH) {
            own = 0;
            if (CreativeScreenAccess.cursed$tab().getType() != CreativeModeTab.Type.SEARCH) {
                acc.cursed$select(CreativeModeTabs.searchTab());
            } else if (button == 1) {
                // the right button empties it
                acc.cursed$search().setValue("");
                acc.cursed$refresh();
            }
            return true;
        }
        if (code == SAVE || code == KIT_NEW) {
            if (button == 0) {
                newKit(s, mc);
            }
            return true;
        }
        if (code >= KIT_TAKE && code < KIT_TAKE + KITS.size()) {
            if (button == 0) {
                putOn(mc, code - KIT_TAKE);
            }
            return true;
        }
        if (code >= KIT_OVER && code < KIT_OVER + KITS.size() || code >= KIT_DEL && code < KIT_DEL + KITS.size()) {
            if (button == 0) {
                int k = code >= KIT_DEL ? code - KIT_DEL : code - KIT_OVER;
                int what = code >= KIT_DEL ? 2 : 1;
                long now = System.nanoTime();
                if (armedKit == k && armedWhat == what && now - armedAt < ARMED_NS) {
                    armedKit = -1;
                    if (what == 1) {
                        overwrite(mc, k);
                    } else {
                        KITS.remove(k);
                        // (a stack from a set: still that set's, wherever it is now)
                        fromKit = fromKit == k ? -1 : fromKit > k ? fromKit - 1 : fromKit;
                        storeKits();
                    }
                } else {
                    armedKit = k;
                    armedWhat = what;
                    armedAt = now;
                }
            }
            return true;
        }
        if (code >= KIT_NAME && code < KIT_NAME + KITS.size()) {
            if (button == 0 && renaming != code - KIT_NAME) {
                renaming = code - KIT_NAME;
                renameBuf = KITS.get(renaming).name;
            }
            return true;
        }
        // the bin on every tab: the carried stack goes; with Shift, everything (as the game's own bin does), for a
        // while to be put back
        if (code == BIN) {
            if (event.hasShiftDown()) {
                empty(mc);
            } else if (!s.getMenu().getCarried().isEmpty()) {
                // taken from a set: it leaves the set
                if (fromKit >= 0 && fromKit < KITS.size() && ItemStack.isSameItemSameComponents(KITS.get(fromKit).items[fromSlot], s.getMenu().getCarried())) {
                    KITS.get(fromKit).items[fromSlot] = ItemStack.EMPTY;
                    storeKits();
                }
                fromKit = -1;
                s.getMenu().setCarried(ItemStack.EMPTY);
                binAt = System.nanoTime();
            }
            return true;
        }
        if (code == TOGGLE) {
            if (button == 0) {
                Native.guiCall("inv\trail");
            }
            return true;
        }
        if (code == UNDO) {
            if (button == 0) {
                putBack(mc);
            }
            return true;
        }
        if (code >= GRID && code < RAIL && !s.isInventoryOpen() && (button == 2 || button == 0 && mc.hasAltDown())) {
            List<ItemStack> list = list(s);
            int k = code - GRID;
            if (k < list.size()) {
                favourite(list.get(k), k);
            }
            return true;
        }
        return false;
    }

    /** Shift on the bin: the whole inventory emptied (armour, the backpack, the hotbar, the other hand), what was in
     *  it kept for the undo. Nothing in it: nothing done. */
    private static void empty(Minecraft mc) {
        LocalPlayer p = mc.player;
        if (p == null || mc.gameMode == null) {
            return;
        }
        var menu = p.inventoryMenu;
        int n = menu.slots.size();
        ItemStack[] was = new ItemStack[n];
        boolean any = false;
        for (int i = 0; i < n; i++) {
            was[i] = menu.getSlot(i).getItem().copy();
            any |= i > 0 && !was[i].isEmpty();
        }
        if (!any) {
            return;
        }
        // (0 is the crafting's result: the game makes it itself)
        for (int i = 1; i < n; i++) {
            if (!was[i].isEmpty()) {
                menu.getSlot(i).set(ItemStack.EMPTY);
                mc.gameMode.handleCreativeModeItemAdd(ItemStack.EMPTY, i);
            }
        }
        cleared = was;
        clearedAt = System.nanoTime();
        clearedKind = 0;
        binAt = clearedAt;
    }

    /** The bin's undo: what the inventory held put back where it was (what came in since stays where it is not in the
     *  way). */
    private static void putBack(Minecraft mc) {
        LocalPlayer p = mc.player;
        ItemStack[] was = cleared;
        if (p == null || mc.gameMode == null || was == null || System.nanoTime() - clearedAt > UNDO_NS) {
            return;
        }
        var menu = p.inventoryMenu;
        for (int i = 1; i < was.length && i < menu.slots.size(); i++) {
            // emptied: back where nothing came since; a set put on: the inventory as it was, all of it
            if (clearedKind == 1 && i >= 5 || !was[i].isEmpty() && menu.getSlot(i).getItem().isEmpty()) {
                menu.getSlot(i).set(was[i].copy());
                mc.gameMode.handleCreativeModeItemAdd(was[i].copy(), i);
            }
        }
        cleared = null;
        clearedAt = 0L;
    }

    // ------------------------------------------------------------------------------------------ the sets

    private static ItemStack kitItem(int code) {
        int k = (code - KIT) / 64, slot = (code - KIT) % 64;
        return k < KITS.size() && slot < 46 ? KITS.get(k).items[slot] : ItemStack.EMPTY;
    }

    /** The inventory as a new set (its name: none, "Set N" till one is typed); the sets shown, it at their top. */
    private static void newKit(CreativeModeInventoryScreen s, Minecraft mc) {
        LocalPlayer p = mc.player;
        if (p == null) {
            return;
        }
        Kit k = new Kit();
        boolean any = false;
        for (int i = 5; i <= 45; i++) {
            k.items[i] = p.inventoryMenu.getSlot(i).getItem().copy();
            any |= !k.items[i].isEmpty();
        }
        if (!any) {
            return;
        }
        KITS.add(0, k);
        // (what pointed at a set by its place follows it down)
        if (fromKit >= 0) {
            fromKit++;
        }
        armedKit = -1;
        storeKits();
        savedAt = System.nanoTime();
        savedSlot = 0;
        for (Row r : ROWS) {
            if (r.kind() == 3) {
                choose(s, r);
            }
        }
    }

    /** A set put on: the inventory as the set has it (what was there kept for the undo). */
    private static void putOn(Minecraft mc, int k) {
        LocalPlayer p = mc.player;
        if (p == null || mc.gameMode == null || k >= KITS.size()) {
            return;
        }
        var menu = p.inventoryMenu;
        int n = menu.slots.size();
        ItemStack[] was = new ItemStack[n];
        for (int i = 0; i < n; i++) {
            was[i] = menu.getSlot(i).getItem().copy();
        }
        Kit kit = KITS.get(k);
        for (int i = 5; i <= 45 && i < n; i++) {
            ItemStack st = kit.items[i].copy();
            if (!ItemStack.matches(st, menu.getSlot(i).getItem())) {
                menu.getSlot(i).set(st);
                mc.gameMode.handleCreativeModeItemAdd(st.copy(), i);
            }
        }
        cleared = was;
        clearedAt = System.nanoTime();
        clearedKind = 1;
    }

    /** A set written over with the inventory as it is now. */
    private static void overwrite(Minecraft mc, int k) {
        LocalPlayer p = mc.player;
        if (p == null || k >= KITS.size()) {
            return;
        }
        Kit kit = KITS.get(k);
        for (int i = 5; i <= 45; i++) {
            kit.items[i] = p.inventoryMenu.getSlot(i).getItem().copy();
        }
        kit.unread.clear();
        storeKits();
        savedAt = System.nanoTime();
        savedSlot = k;
    }

    private static void endRename(boolean keep) {
        if (renaming >= 0 && renaming < KITS.size() && keep) {
            KITS.get(renaming).name = renameBuf.strip();
            storeKits();
        }
        renaming = -1;
    }

    /** A key while a set's name is typed: all of them are the name's (Enter keeps it, Esc does not). */
    public static boolean key(net.minecraft.client.input.KeyEvent event) {
        if (renaming >= 0 && own != 6) {
            endRename(true);
        }
        if (renaming < 0) {
            return false;
        }
        int k = event.key();
        if (k == com.mojang.blaze3d.platform.InputConstants.KEY_BACKSPACE && !renameBuf.isEmpty()) {
            renameBuf = renameBuf.substring(0, renameBuf.offsetByCodePoints(renameBuf.length(), -1));
        } else if (k == com.mojang.blaze3d.platform.InputConstants.KEY_RETURN || k == com.mojang.blaze3d.platform.InputConstants.KEY_NUMPADENTER) {
            endRename(true);
        } else if (event.isEscape()) {
            endRename(false);
        }
        return true;
    }

    public static boolean typed(int codepoint) {
        if (renaming >= 0 && own != 6) {
            endRename(true);
        }
        if (renaming < 0) {
            return false;
        }
        if (!Character.isISOControl(codepoint) && renameBuf.codePointCount(0, renameBuf.length()) < 32) {
            renameBuf += new String(Character.toChars(codepoint));
        }
        return true;
    }

    private static Path kitsFile() {
        return Minecraft.getInstance().gameDirectory.toPath().resolve("cursed-recode").resolve("kits.txt");
    }

    /** The sets from their file: "kit<TAB>name", then "item<TAB>slot<TAB>the stack as the game writes it". */
    private static void loadKits(Minecraft mc) {
        try {
            Path f = kitsFile();
            if (!Files.exists(f) || mc.level == null) {
                return;
            }
            var ops = mc.level.registryAccess().createSerializationContext(JsonOps.INSTANCE);
            Kit cur = null;
            for (String line : Files.readAllLines(f, StandardCharsets.UTF_8)) {
                String[] p = line.split("\t", 3);
                if (p[0].equals("kit")) {
                    cur = new Kit();
                    cur.name = p.length > 1 ? p[1] : "";
                    KITS.add(cur);
                } else if (p[0].equals("item") && p.length == 3 && cur != null) {
                    int slot = -1;
                    try {
                        slot = Integer.parseInt(p[1]);
                        Kit k = cur;
                        int at = slot;
                        if (slot >= 5 && slot <= 45) {
                            ItemStack.CODEC.parse(ops, JsonParser.parseString(p[2])).result().ifPresent(st -> k.items[at] = st);
                        }
                    } catch (Exception ignored) {
                        // read below as unread
                    }
                    if (slot >= 5 && slot <= 45 && cur.items[slot].isEmpty()) {
                        cur.unread.put(slot, p[2]);
                    }
                }
            }
        } catch (Exception e) {
            org.slf4j.LoggerFactory.getLogger("HudCreative").warn("Could not read kits.txt", e);
        }
    }

    private static void storeKits() {
        Minecraft mc = Minecraft.getInstance();
        if (mc.level == null) {
            return;
        }
        try {
            var ops = mc.level.registryAccess().createSerializationContext(JsonOps.INSTANCE);
            StringBuilder b = new StringBuilder();
            for (Kit k : KITS) {
                b.append("kit\t").append(k.name.replace('\t', ' ').replace('\n', ' ')).append('\n');
                for (int i = 5; i <= 45; i++) {
                    if (!k.items[i].isEmpty()) {
                        int slot = i;
                        ItemStack.CODEC.encodeStart(ops, k.items[i]).result().ifPresent(j -> b.append("item\t").append(slot).append('\t').append(j).append('\n'));
                    } else if (k.unread.containsKey(i)) {
                        b.append("item\t").append(i).append('\t').append(k.unread.get(i)).append('\n');
                    }
                }
            }
            Path f = kitsFile();
            Files.createDirectories(f.getParent());
            Files.writeString(f, b.toString(), StandardCharsets.UTF_8);
        } catch (Exception e) {
            org.slf4j.LoggerFactory.getLogger("HudCreative").warn("Could not write kits.txt", e);
        }
    }

    /** A drag the screen's own (a block held by its header, a slider of the settings). */
    public static boolean takeDrag() {
        return swallowed;
    }

    /** A button let go: the screen's own when its press was. */
    public static boolean takeRelease(int button) {
        held &= ~(button == 0 ? 1 : button == 1 ? 2 : 0);
        boolean take = swallowed;
        swallowed = false;
        return take;
    }

    /** Whether the settings' panel takes the keys and the letters (the colour wheel's hex being typed in it). */
    public static boolean panelKeys() {
        return panelKeys;
    }

    /** Esc while the settings are open: they close (a list or the colour wheel first), the screen stays. */
    public static boolean escape() {
        if (panelOpen) {
            ask = 2;
            return true;
        }
        return false;
    }

    /** A row of the rail chosen: the game's tab, or one of the client's lists over a tab of items. */
    private static void choose(CreativeModeInventoryScreen s, Row row) {
        var acc = (CreativeScreenAccess) s;
        CreativeModeTab cur = CreativeScreenAccess.cursed$tab();
        if (row.kind() == 1 || row.kind() == 2 || row.kind() == 3) {
            // the game's picker clicks need a tab of items under it (not the search's field, not the inventory)
            if (cur.getType() != CreativeModeTab.Type.CATEGORY) {
                acc.cursed$select(CreativeModeTabs.getDefaultTab());
            }
            own = row.kind() == 1 ? 4 : row.kind() == 2 ? 5 : 6;
            ownOn = CreativeScreenAccess.cursed$tab();
        } else if (row.tab() != null) {
            own = 0;
            if (row.tab() != cur) {
                acc.cursed$select(row.tab());
            }
        }
    }

    /** The wheel over the screen turns the grid (not over a slot: a bundle there takes it). */
    public static boolean scroll(CreativeModeInventoryScreen s, double x, double y, double dy) {
        // the settings under the pointer scroll
        if (panelOpen && Native.hudScroll((float) dy)) {
            return true;
        }
        int code = codeAt((float) (x * gs), (float) (y * gs));
        if (code >= 0 && code < BIN) {
            return false;
        }
        wheel += dy;
        return true;
    }

    /** The game's click on its picker's first slot (a grid item): a stack to the pointer, a single one with the right
     *  button, Shift a stack straight into the hotbar, a number key into that slot (the game puts it), the pick key a
     *  favourite; what is taken is remembered. True: done here. */
    public static boolean slotClicked(CreativeModeInventoryScreen s, @Nullable Slot slot, int button, ContainerInput input) {
        Minecraft mc = Minecraft.getInstance();
        LocalPlayer p = mc.player;
        if (!active(s) || slot == null || p == null) {
            return false;
        }
        var menu = s.getMenu();
        if (s.isInventoryOpen()) {
            if (slot == ((CreativeScreenAccess) s).cursed$bin() && !menu.getCarried().isEmpty()) {
                binAt = System.nanoTime();
            }
            return false;
        }
        if (menu.slots.isEmpty() || slot != menu.slots.get(0)) {
            return false;
        }
        ItemStack st = slot.getItem();
        if (st.isEmpty() || !slot.mayPickup(p) || !menu.getCarried().isEmpty()) {
            return false;
        }
        // a set's item: as it is in the set (the right button: one of it); let go in the bin, it leaves the set
        if (lastKit >= KIT && input == ContainerInput.PICKUP) {
            menu.setCarried(button == 1 ? st.copyWithCount(1) : st.copy());
            fromKit = (lastKit - KIT) / 64;
            fromSlot = (lastKit - KIT) % 64;
            return true;
        }
        switch (input) {
            case PICKUP -> {
                menu.setCarried(st.copyWithCount(button == 1 ? 1 : st.getMaxStackSize()));
                remember(st);
                return true;
            }
            case QUICK_MOVE -> {
                var inv = p.getInventory();
                int to = -1;
                for (int i = 0; i < 9 && to < 0; i++) {
                    if (inv.getItem(i).isEmpty()) {
                        to = i;
                    }
                }
                if (to < 0) {
                    to = inv.getSelectedSlot();
                }
                inv.setItem(to, st.copyWithCount(st.getMaxStackSize()));
                p.inventoryMenu.broadcastChanges();
                remember(st);
                fly(st, to);
                return true;
            }
            case SWAP -> {
                if (button >= 0 && button < 9) {
                    remember(st);
                    fly(st, button);
                }
                return false;
            }
            case CLONE -> {
                favourite(st, lastGrid >= GRID ? lastGrid - GRID : -1);
                return true;
            }
            default -> {
                return false;
            }
        }
    }

    private static void fly(ItemStack st, int to) {
        float[] from = lastGrid >= GRID ? iconOf(lastGrid) : null;
        if (from == null) {
            return;
        }
        System.arraycopy(from, 0, FLY_FROM, 0, 3);
        flyStack = st.copyWithCount(1);
        flyTo = to;
        flyAt = System.nanoTime();
        landed = -1;
    }

    // ------------------------------------------------------------------------------------------ favourites, the recent

    private static int indexOf(List<ItemStack> l, ItemStack s) {
        for (int i = 0; i < l.size(); i++) {
            if (ItemStack.isSameItemSameComponents(l.get(i), s)) {
                return i;
            }
        }
        return -1;
    }

    private static void favourite(ItemStack st, int place) {
        int i = indexOf(FAV, st);
        if (i >= 0) {
            FAV.remove(i);
        } else {
            FAV.add(st.copyWithCount(1));
        }
        pulsed = place;
        pulseAt = System.nanoTime();
        store();
    }

    private static void remember(ItemStack st) {
        int i = indexOf(RECENT, st);
        if (i >= 0) {
            RECENT.remove(i);
        }
        RECENT.add(0, st.copyWithCount(1));
        while (RECENT.size() > 40) {
            RECENT.remove(RECENT.size() - 1);
        }
        store();
    }

    private static Path file() {
        return Minecraft.getInstance().gameDirectory.toPath().resolve("cursed-recode").resolve("creative.txt");
    }

    /** Favourites and the recently taken from the file (each stack as the game writes it: its components too). */
    private static void load(Minecraft mc) {
        if (loaded || mc.level == null) {
            return;
        }
        loaded = true;
        loadKits(mc);
        try {
            Path f = file();
            if (!Files.exists(f)) {
                return;
            }
            var ops = mc.level.registryAccess().createSerializationContext(JsonOps.INSTANCE);
            for (String line : Files.readAllLines(f, StandardCharsets.UTF_8)) {
                int tab = line.indexOf('\t');
                if (tab < 0) {
                    continue;
                }
                String kind = line.substring(0, tab);
                try {
                    ItemStack.CODEC.parse(ops, JsonParser.parseString(line.substring(tab + 1))).result().ifPresent(st -> {
                        if (kind.equals("fav")) {
                            FAV.add(st);
                        } else if (kind.equals("recent")) {
                            RECENT.add(st);
                        }
                    });
                } catch (Exception ignored) {
                    // an item the game has no more
                }
            }
        } catch (Exception e) {
            org.slf4j.LoggerFactory.getLogger("HudCreative").warn("Could not read creative.txt", e);
        }
    }

    private static void store() {
        Minecraft mc = Minecraft.getInstance();
        if (mc.level == null) {
            return;
        }
        try {
            var ops = mc.level.registryAccess().createSerializationContext(JsonOps.INSTANCE);
            StringBuilder b = new StringBuilder();
            for (ItemStack st : FAV) {
                ItemStack.CODEC.encodeStart(ops, st).result().ifPresent(j -> b.append("fav\t").append(j).append('\n'));
            }
            for (ItemStack st : RECENT) {
                ItemStack.CODEC.encodeStart(ops, st).result().ifPresent(j -> b.append("recent\t").append(j).append('\n'));
            }
            Path f = file();
            Files.createDirectories(f.getParent());
            Files.writeString(f, b.toString(), StandardCharsets.UTF_8);
        } catch (Exception e) {
            org.slf4j.LoggerFactory.getLogger("HudCreative").warn("Could not write creative.txt", e);
        }
    }

    // ------------------------------------------------------------------------------------------ what the game knows

    /** The grid's list: the client's own, or the game's tab's (its search's results). */
    private static List<ItemStack> list(CreativeModeInventoryScreen s) {
        if (own == 6) {
            return List.of();
        }
        if (own == 4) {
            return FAV;
        }
        if (own == 5) {
            return RECENT;
        }
        return s.getMenu().items;
    }

    private static int tabKind(CreativeModeInventoryScreen s) {
        if (own != 0) {
            return own;
        }
        return switch (CreativeScreenAccess.cursed$tab().getType()) {
            case SEARCH -> 1;
            case HOTBAR -> 2;
            case INVENTORY -> 3;
            default -> 0;
        };
    }

    private static void rows(Minecraft mc) {
        ROWS.clear();
        CreativeModeTab cur = CreativeScreenAccess.cursed$tab();
        ROWS.add(new Row(1, null, FAV.size(), false, own == 4, ""));
        ROWS.add(new Row(2, null, RECENT.size(), false, own == 5, ""));
        ROWS.add(new Row(3, null, KITS.size(), false, own == 6, ""));
        CreativeModeTab hotbars = null, inventory = null;
        boolean first = true;
        for (CreativeModeTab t : CreativeModeTabs.tabs()) {
            switch (t.getType()) {
                case CATEGORY -> {
                    ROWS.add(new Row(0, t, t.getDisplayItems().size(), first, own == 0 && t == cur, HudInv.clean(t.getDisplayName().getString())));
                    first = false;
                }
                case HOTBAR -> hotbars = t;
                case INVENTORY -> inventory = t;
                default -> {
                }
            }
        }
        if (hotbars != null) {
            int saved = 0;
            for (int i = 0; i < 9; i++) {
                if (!mc.getHotbarManager().get(i).isEmpty()) {
                    saved++;
                }
            }
            ROWS.add(new Row(0, hotbars, saved, true, own == 0 && hotbars == cur, HudInv.clean(hotbars.getDisplayName().getString())));
        }
        if (inventory != null) {
            ROWS.add(new Row(0, inventory, -1, hotbars == null, own == 0 && inventory == cur, HudInv.clean(inventory.getDisplayName().getString())));
        }
    }

    private static boolean isFav(ItemStack s) {
        return indexOf(FAV, s) >= 0;
    }

    private static double since(long now, long at) {
        return at == 0 ? -1.0 : (now - at) / 1e9;
    }

    private static double[] facts(Minecraft mc, LocalPlayer p, CreativeModeInventoryScreen screen, StringBuilder text) {
        List<Double> n = new ArrayList<>(2048);
        var menu = screen.getMenu();
        var acc = (ContainerScreenAccess) screen;
        long now = System.nanoTime();
        List<ItemStack> list = list(screen);
        CreativeModeTab tab = CreativeScreenAccess.cursed$tab();
        int kind = tabKind(screen);
        n.add((double) gs);
        n.add((now - openAt) / 1e9);
        n.add(mc.mouseHandler.xpos());
        n.add(mc.mouseHandler.ypos());
        n.add((double) p.getInventory().getSelectedSlot());
        n.add((double) menu.getCarried().getCount());
        n.add((double) kind);
        n.add((double) (own != 0 ? -own : System.identityHashCode(tab)));
        n.add(wheel);
        wheel = 0.0;
        // (the sets have no end: the saving is always there)
        n.add(0.0);
        n.add(since(now, savedAt));
        n.add((double) savedSlot);
        n.add(since(now, binAt));
        // the grid's items near what is seen
        int size = kind == 3 ? 0 : list.size();
        int i0 = Math.min(want0, size), i1 = Math.min(Math.min(want1, size), i0 + 1024);
        n.add((double) size);
        n.add((double) i0);
        n.add((double) (i1 - i0));
        for (int i = i0; i < i1; i++) {
            ItemStack s = list.get(i);
            n.add(s.isEmpty() ? 0.0 : (double) HudInv.colour(mc, s));
            n.add(s.isEmpty() ? 0.0 : (double) HudInv.nameColour(s));
            int fl = (s.isEmpty() || !isFav(s) ? 0 : 1) | (s.hasFoil() ? 2 : 0) | (s.has(DataComponents.CREATIVE_SLOT_LOCK) ? 4 : 0);
            n.add((double) fl);
            n.add((double) s.getCount());
        }
        // the menu's slots (the hotbar's; the survival inventory's on its tab)
        var slots = menu.slots;
        boolean quick = acc.cursed$quick();
        var quickSlots = acc.cursed$quickSlots();
        n.add((double) slots.size());
        for (Slot sl : slots) {
            HudInv.slot(n, mc, sl.getItem(), quick && quickSlots.contains(sl));
        }
        // the rail
        rows(mc);
        n.add((double) ROWS.size());
        for (Row r : ROWS) {
            n.add((double) r.kind());
            n.add((double) r.count());
            n.add(r.sep() ? 1.0 : 0.0);
            n.add(r.on() ? 1.0 : 0.0);
        }
        // the inspected item: the one pointed at (kept a moment over the gaps), else the one in the hand
        int code = codeAt((float) mc.mouseHandler.xpos(), (float) mc.mouseHandler.ypos());
        ItemStack hov = ItemStack.EMPTY;
        if (code >= GRID && code < RAIL && code - GRID < list.size() && kind != 3) {
            hov = list.get(code - GRID);
        } else if (code >= 0 && code < BIN && code < slots.size()) {
            hov = slots.get(code).getItem();
        } else if (code >= KIT && code < KIT + 64 * KITS.size()) {
            hov = kitItem(code);
        }
        if (!hov.isEmpty()) {
            pointed = hov;
            pointedAt = now;
        } else if (!pointed.isEmpty() && now - pointedAt < 250_000_000L && code < 0) {
            hov = pointed;
        }
        ItemStack ins = !hov.isEmpty() ? hov : p.getInventory().getSelectedItem();
        inspected = menu.getCarried().isEmpty() ? ins : ItemStack.EMPTY;
        StringBuilder words = new StringBuilder();
        HudInv.inspected(n, words, mc, inspected, BOXED);
        // the words
        if (kind != 4 && kind != 5) {
            text.append("title\t").append(HudInv.clean(tab.getDisplayName().getString())).append('\n');
        }
        EditBox box = ((CreativeScreenAccess) screen).cursed$search();
        if (box != null && kind == 1) {
            text.append("q\t").append(HudInv.clean(box.getValue())).append('\t').append(box.getCursorPosition()).append('\t').append(box.isFocused() ? '1' : '0').append('\n');
        }
        if (cleared != null && now - clearedAt < UNDO_NS) {
            text.append("cleared\t").append(since(now, clearedAt)).append('\t').append(clearedKind).append('\n');
        } else {
            cleared = null;
        }
        // the sets: each its name and how many of each place it holds (the player's menu's 5…45)
        for (Kit k : KITS) {
            StringBuilder c = new StringBuilder();
            for (int i = 5; i <= 45; i++) {
                c.append(i > 5 ? "," : "").append(k.items[i].getCount());
            }
            text.append("kit\t").append(HudInv.clean(k.name)).append('\t').append(c).append('\n');
        }
        if (armedKit >= 0 && now - armedAt < ARMED_NS) {
            text.append("kitarm\t").append(armedKit).append('\t').append(armedWhat).append('\n');
        }
        if (renaming >= 0) {
            text.append("kitname\t").append(renaming).append('\t').append(HudInv.clean(renameBuf)).append('\n');
        }
        // (the stack from a set let go elsewhere: no more the set's)
        if (menu.getCarried().isEmpty()) {
            fromKit = -1;
        } else if (fromKit >= 0) {
            text.append("fromkit\t1\n");
        }
        text.append("shift\t").append(mc.hasShiftDown() ? '1' : '0').append('\n');
        text.append("mouse\t").append(held | pressed).append('\t').append(ask).append('\n');
        pressed = 0;
        ask = 0;
        text.append("keys\t").append(HudInv.clean(mc.options.keySaveHotbarActivator.getTranslatedKeyMessage().getString())).append('\t')
                .append(HudInv.clean(mc.options.keyLoadHotbarActivator.getTranslatedKeyMessage().getString())).append('\n');
        for (Row r : ROWS) {
            text.append("rail\t").append(r.name()).append('\n');
        }
        text.append(words);
        double[] out = new double[n.size()];
        for (int i = 0; i < out.length; i++) {
            out[i] = n.get(i);
        }
        return out;
    }
}
