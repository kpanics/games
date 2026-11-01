package cursed.recode.hud;

import cursed.recode.mixin.client.RecipesAccess;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.inventory.AbstractContainerScreen;
import net.minecraft.client.gui.screens.inventory.AbstractRecipeBookScreen;
import net.minecraft.client.gui.screens.recipebook.RecipeBookComponent;
import net.minecraft.client.gui.screens.recipebook.RecipeCollection;
import net.minecraft.client.gui.screens.recipebook.SearchRecipeBookCategory;
import net.minecraft.network.chat.Component;
import net.minecraft.world.entity.player.StackedItemContents;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.crafting.display.RecipeDisplayEntry;
import net.minecraft.world.item.crafting.display.SlotDisplayContext;
import org.jspecify.annotations.Nullable;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;

/**
 * Custom Inventory's recipe book (her choice, 15.10: kept, in the client's look, a block of its own on the left, its
 * button a quiet icon). The game's book is not shown (RecipeBookComponentMixin); this reads what it knows — its tabs,
 * its collections, which can be made from what is at hand — and sends the native the block's contents (native
 * hudpack Rec). A recipe clicked is put in the crafting grid by the server, as the game's book asks it; clicked again,
 * the next way of making it.
 */
public final class HudRecipes {
    private HudRecipes() {
    }

    /** The screen these are for; the tab chosen; the words searched and whether the field has the keys. */
    private static @Nullable Screen of;
    private static int tab;
    private static String query = "";
    private static boolean focused;
    /** The wheel turned over the block since the last frame. */
    private static double wheel;
    /** The tabs shown (the book's, those with recipes), the recipes shown and their pictures. */
    private static final List<RecipeBookComponent.TabInfo> TABS = new ArrayList<>();
    private static final List<RecipeCollection> SHOWN = new ArrayList<>();
    private static final List<ItemStack> ICONS = new ArrayList<>();
    /** Which way of making a collection comes next when it is clicked again. */
    private static final Map<RecipeCollection, Integer> NEXT = new HashMap<>();
    private static final StackedItemContents STACKED = new StackedItemContents();
    private static long matchedAt;
    private static int changedWas = -1;
    private static String shownFor = "";

    /** The native's last frame: where OUT's recipe part is. */
    private static float[] out = new float[0];
    private static int at = -1;

    /** The screen's book (the inventory's, a crafting table's, a furnace's), if it has one. */
    private static @Nullable RecipeBookComponent<?> book(@Nullable Screen screen) {
        return screen instanceof AbstractRecipeBookScreen<?> rs ? ((RecipesAccess.Screen) rs).cursed$book() : null;
    }

    static boolean focused() {
        return focused;
    }

    /** The recipe the server could not fill (the game's book's ghost): what each slot is missing, an item a second in
     *  turn when it may be several; none: an empty map. */
    static java.util.Map<net.minecraft.world.inventory.Slot, ItemStack> ghosts(@Nullable Screen screen) {
        RecipeBookComponent<?> comp = book(screen);
        if (comp == null) {
            return java.util.Map.of();
        }
        var map = ((RecipesAccess.Ghosts) ((RecipesAccess.Book) comp).cursed$ghosts()).cursed$ingredients();
        if (map.isEmpty()) {
            return java.util.Map.of();
        }
        java.util.Map<net.minecraft.world.inventory.Slot, ItemStack> out = new java.util.HashMap<>();
        int turn = (int) (System.currentTimeMillis() / 1000L);
        for (var e : map.reference2ObjectEntrySet()) {
            List<ItemStack> items = ((RecipesAccess.Ghost) e.getValue()).cursed$items();
            if (!items.isEmpty()) {
                out.put(e.getKey(), items.get(Math.floorMod(turn, items.size())));
            }
        }
        return out;
    }

    // ------------------------------------------------------------------------------------------ what the native gets

    /** [there is one, open, only what can be made, tabs T then 1 for the chosen, recipes K then (can be made now,
     *  more than one way), the wheel]; the text's last line "rq\twords\tfocused". */
    static void facts(Minecraft mc, AbstractContainerScreen<?> screen, List<Double> n, StringBuilder text) {
        RecipeBookComponent<?> comp = book(screen);
        if (comp == null || mc.player == null || mc.level == null) {
            n.add(0.0);
            return;
        }
        if (screen != of) {
            of = screen;
            tab = 0;
            query = "";
            focused = false;
            NEXT.clear();
            changedWas = -1;
            at = -1;
            out = new float[0];
            shownFor = "";
        }
        var acc = (RecipesAccess.Book) comp;
        var type = acc.cursed$menu().getRecipeBookType();
        var cb = mc.player.getRecipeBook();
        boolean open = cb.isOpen(type), filtering = cb.isFiltering(type);
        n.add(1.0);
        n.add(open ? 1.0 : 0.0);
        n.add(filtering ? 1.0 : 0.0);
        if (!open) {
            TABS.clear();
            SHOWN.clear();
            ICONS.clear();
            shownFor = "";
        }
        if (open) {
            // what can be made from what is at hand: again when the inventory changes, and a few times a second (the
            // crafting grid's own slots)
            long now = System.nanoTime();
            int changed = mc.player.getInventory().getTimesChanged();
            if (changed != changedWas || now - matchedAt > 1_000_000_000L) {
                changedWas = changed;
                matchedAt = now;
                STACKED.clear();
                mc.player.getInventory().fillStackedContents(STACKED);
                acc.cursed$menu().fillCraftSlotsStackedContents(STACKED);
                for (RecipeBookComponent.TabInfo ti : acc.cursed$tabs()) {
                    for (RecipeCollection c : cb.getCollection(ti.category())) {
                        acc.cursed$select(c, STACKED);
                    }
                }
            }
            String key = tab + "|" + query + "|" + filtering + "|" + matchedAt + "|" + NEXT.hashCode();
            boolean again = !key.equals(shownFor);
            shownFor = key;
            if (again) {
                TABS.clear();
                SHOWN.clear();
                ICONS.clear();
                for (RecipeBookComponent.TabInfo ti : acc.cursed$tabs()) {
                    if (ti.category() instanceof SearchRecipeBookCategory || cb.getCollection(ti.category()).stream().anyMatch(RecipeCollection::hasAnySelected)) {
                        TABS.add(ti);
                    }
                }
                tab = Math.max(0, Math.min(tab, TABS.size() - 1));
            }
            if (again && !TABS.isEmpty()) {
                List<RecipeCollection> list = new ArrayList<>(cb.getCollection(TABS.get(tab).category()));
                list.removeIf(c -> !c.hasAnySelected());
                if (!query.isEmpty() && mc.getConnection() != null) {
                    Set<RecipeCollection> found = new HashSet<>(mc.getConnection().searchTrees().recipes().search(query.toLowerCase(Locale.ROOT)));
                    list.removeIf(c -> !found.contains(c));
                }
                if (filtering) {
                    list.removeIf(c -> !c.hasCraftable());
                }
                var ctx = SlotDisplayContext.fromLevel(mc.level);
                for (RecipeCollection c : list) {
                    List<RecipeDisplayEntry> ways = ways(c);
                    ItemStack icon = ItemStack.EMPTY;
                    if (!ways.isEmpty()) {
                        int k = NEXT.getOrDefault(c, 0) % ways.size();
                        List<ItemStack> res = ways.get(k).resultItems(ctx);
                        icon = res.isEmpty() ? ItemStack.EMPTY : res.get(0);
                    }
                    SHOWN.add(c);
                    ICONS.add(icon);
                }
            }
        }
        n.add((double) TABS.size());
        for (int i = 0; i < TABS.size(); i++) {
            n.add(i == tab ? 1.0 : 0.0);
        }
        n.add((double) SHOWN.size());
        for (RecipeCollection c : SHOWN) {
            n.add(c.hasCraftable() ? 1.0 : 0.0);
            n.add(c.getSelectedRecipes(RecipeCollection.CraftableStatus.ANY).size() > 1 ? 1.0 : 0.0);
        }
        n.add(wheel);
        wheel = 0.0;
        text.append("rq\t").append(HudInv.clean(query)).append('\t').append(focused ? '1' : '0').append('\n');
    }

    /** A collection's ways of making it: those that can be made now first (or only those), the rest after. */
    private static List<RecipeDisplayEntry> ways(RecipeCollection c) {
        List<RecipeDisplayEntry> can = c.getSelectedRecipes(RecipeCollection.CraftableStatus.CRAFTABLE);
        List<RecipeDisplayEntry> not = c.getSelectedRecipes(RecipeCollection.CraftableStatus.NOT_CRAFTABLE);
        List<RecipeDisplayEntry> all = new ArrayList<>(can);
        all.addAll(not);
        return all;
    }

    /** The recipe pointed at in the last frame: its result's tooltip (the game's lines), else null. */
    static @Nullable List<Component> tip(Minecraft mc) {
        int k = pointed();
        if (k < 0 || k >= ICONS.size() || ICONS.get(k).isEmpty()) {
            return null;
        }
        return Screen.getTooltipFromItem(mc, ICONS.get(k));
    }

    private static int pointed() {
        if (at < 0 || out.length <= at || out[at] < 0.5F) {
            return -1;
        }
        int o = at + 5;
        if (out[o] < 0.5F) {
            return -1;
        }
        o += 1 + 16;
        int t = (int) out[o];
        o += 1 + t * 4;
        int k = (int) out[o];
        o += 1 + k * 9;
        return (int) out[o];
    }

    // ------------------------------------------------------------------------------------------ the frame

    /** Where the native put the book (OUT from `o`): the tabs' pictures and the recipes' items drawn there. */
    static void draw(GuiGraphicsExtractor graphics, float[] o0, int o, float gs) {
        out = o0;
        at = o;
        if (o >= o0.length || o0[o] < 0.5F) {
            at = -1;
            return;
        }
        int q = o + 5;
        if (o0[q] < 0.5F) {
            return;
        }
        float gx0 = o0[q + 13], gy0 = o0[q + 14], gx1 = o0[q + 15], gy1 = o0[q + 16];
        q += 17;
        int t = (int) o0[q++];
        for (int i = 0; i < t; i++, q += 4) {
            if (i < TABS.size()) {
                HudInv.item(graphics, TABS.get(i).primaryIcon(), o0[q] / gs, o0[q + 1] / gs, o0[q + 2] / gs, 1.0F, 5000 + i);
            }
        }
        int k = (int) o0[q++];
        graphics.enableScissor((int) Math.floor(gx0 / gs), (int) Math.floor(gy0 / gs), (int) Math.ceil(gx1 / gs), (int) Math.ceil(gy1 / gs));
        for (int i = 0; i < k; i++, q += 9) {
            int idx = (int) o0[q];
            if (idx >= 0 && idx < ICONS.size() && !ICONS.get(idx).isEmpty() && o0[q + 4] > 0.02F) {
                HudInv.item(graphics, ICONS.get(idx), o0[q + 1] / gs, o0[q + 2] / gs, o0[q + 3] / gs, 1.0F, 6000 + idx);
            }
        }
        graphics.disableScissor();
    }

    // ------------------------------------------------------------------------------------------ the player's doing

    private static boolean in(float[] r, int o, float x, float y) {
        return x >= r[o] && x < r[o + 2] && y >= r[o + 1] && y < r[o + 3];
    }

    /** Over the book's button or its block (window px). */
    static boolean over(float x, float y) {
        if (at < 0 || out.length <= at + 10 || out[at] < 0.5F) {
            return false;
        }
        return in(out, at + 1, x, y) || out[at + 5] > 0.5F && in(out, at + 6, x, y);
    }

    /** A click (window px) on the button, a tab, the field, the filter or a recipe: done here. */
    static boolean click(Minecraft mc, Screen screen, float x, float y, int button, boolean shift) {
        RecipeBookComponent<?> comp = book(screen);
        if (comp == null || mc.player == null || !over(x, y)) {
            if (focused && comp != null) {
                focused = false;
            }
            return false;
        }
        var acc = (RecipesAccess.Book) comp;
        var type = acc.cursed$menu().getRecipeBookType();
        var cb = mc.player.getRecipeBook();
        if (in(out, at + 1, x, y)) {
            cb.setOpen(type, !cb.isOpen(type));
            acc.cursed$send();
            focused = false;
            tick(mc);
            return true;
        }
        int q = at + 5;
        if (out[q] < 0.5F) {
            return true;
        }
        boolean field = in(out, q + 5, x, y);
        focused = field;
        if (field) {
            return true;
        }
        if (in(out, q + 9, x, y)) {
            cb.setFiltering(type, !cb.isFiltering(type));
            acc.cursed$send();
            tick(mc);
            return true;
        }
        q += 17;
        int t = (int) out[q++];
        for (int i = 0; i < t; i++, q += 4) {
            float s = out[q + 2];
            float mg = s * 0.2F + 3.0F;
            if (x >= out[q] - mg && x < out[q] + s + mg && y >= out[q + 1] - mg && y < out[q + 1] + s + mg) {
                if (tab != i) {
                    tab = i;
                    tick(mc);
                }
                return true;
            }
        }
        int k = (int) out[q++];
        for (int i = 0; i < k; i++, q += 9) {
            if (in(out, q + 5, x, y)) {
                int idx = (int) out[q];
                if (idx >= 0 && idx < SHOWN.size() && button == 0) {
                    place(mc, SHOWN.get(idx), shift);
                }
                return true;
            }
        }
        return true;
    }

    /** A recipe into the crafting grid (the server moves the items); clicked again, the next way of making it. */
    private static void place(Minecraft mc, RecipeCollection c, boolean shift) {
        List<RecipeDisplayEntry> ways = ways(c);
        if (ways.isEmpty() || mc.gameMode == null || mc.player == null) {
            return;
        }
        int k = NEXT.getOrDefault(c, 0) % ways.size();
        mc.gameMode.handlePlaceRecipe(mc.player.containerMenu.containerId, ways.get(k).id(), shift);
        if (ways.size() > 1) {
            NEXT.put(c, k + 1);
        }
        tick(mc);
    }

    /** The quiet click of the client's buttons. */
    private static void tick(Minecraft mc) {
        mc.getSoundManager().play(net.minecraft.client.resources.sounds.SimpleSoundInstance.forUI(net.minecraft.sounds.SoundEvents.UI_BUTTON_CLICK.value(), 1.6F, 0.25F));
    }

    /** The wheel over the block turns its recipes. */
    static boolean scroll(float x, float y, double dy) {
        if (at < 0 || out.length <= at + 10 || out[at + 5] < 0.5F || !in(out, at + 6, x, y)) {
            return false;
        }
        wheel += dy;
        return true;
    }

    /** A key while the field has them: Backspace, Esc and Enter let it go; the rest stays here (not the game's). */
    static boolean key(int key, boolean escape, boolean enter) {
        if (!focused) {
            return false;
        }
        if (escape || enter) {
            focused = false;
        } else if (key == com.mojang.blaze3d.platform.InputConstants.KEY_BACKSPACE) {
            if (!query.isEmpty()) {
                query = query.substring(0, query.offsetByCodePoints(query.length(), -1));
            }
        }
        return true;
    }

    static boolean typed(int codepoint) {
        if (!focused || Character.isISOControl(codepoint) || query.length() > 40) {
            return focused;
        }
        query += new String(Character.toChars(codepoint));
        return true;
    }
}
