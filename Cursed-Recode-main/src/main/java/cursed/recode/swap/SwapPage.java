package cursed.recode.swap;

import cursed.recode.render.Native;
import java.util.HashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.client.renderer.RenderPipelines;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.world.entity.EquipmentSlot;
import net.minecraft.world.inventory.InventoryMenu;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;

/**
 * Auto Swap's editor in the menu (native gui/swapui.rs): while the menu is open the inventory goes to it — each thing,
 * the kinds it is, what each swap would take now (its arrow) — and its things are drawn where it says, over the
 * menu's picture, shrinking into their middle as their part fades.
 */
public final class SwapPage {
    /** The kinds a thing may be (gui/swap.rs KINDS but "same", which is about what is worn). */
    private static final String[] KINDS = {"totem", "talisman", "shield", "sphere", "firework", "golden_apple", "enchanted_apple", "food", "elytra", "helmet",
            "chestplate", "leggings", "boots"};
    private static final Things.Pick[] KIND_PICKS = new Things.Pick[KINDS.length];
    /** The places' empty marks (gui/swap.rs PLACES' order). */
    private static final Identifier[] EMPTY = {InventoryMenu.EMPTY_ARMOR_SLOT_SHIELD, InventoryMenu.EMPTY_ARMOR_SLOT_HELMET, InventoryMenu.EMPTY_ARMOR_SLOT_CHESTPLATE,
            InventoryMenu.EMPTY_ARMOR_SLOT_LEGGINGS, InventoryMenu.EMPTY_ARMOR_SLOT_BOOTS};
    private static final Map<String, ItemStack> SHOWN = new HashMap<>();
    private static String sent = "";
    private static String json = "";
    private static List<AutoSwap.Swap> swaps = List.of();

    static {
        for (int i = 0; i < KINDS.length; i++) {
            KIND_PICKS[i] = new Things.Pick(null, null, null, KINDS[i]);
        }
    }

    private SwapPage() {
    }

    /** Each tick the menu is open (ClickGui): the inventory, when it changed. */
    public static void tick(Minecraft mc) {
        LocalPlayer p = mc.player;
        if (p == null) {
            return;
        }
        String js = Native.guiCall("swaps");
        if (js != null && !js.equals(json)) {
            json = js;
            swaps = AutoSwap.read(js);
        }
        String text = snapshot(p, AutoSwap.reach(Servers.now(mc)));
        if (!text.equals(sent)) {
            sent = text;
            Native.guiCall("swapinv\t\t" + text);
        }
    }

    /** The menu closed: the next one is sent the inventory anew. */
    public static void closed() {
        sent = "";
    }

    private static String snapshot(LocalPlayer p, Moves.Reach reach) {
        StringBuilder b = new StringBuilder(2048);
        // where things are taken from: 0 anywhere, 1 only the hotbar, 2 FunTime's (the inventory's only totems, spheres)
        b.append("h\t").append(reach == Moves.Reach.HOTBAR ? 1 : reach == Moves.Reach.FUNTIME ? 2 : 0).append('\n');
        for (int i = 0; i < 36; i++) {
            thing(b, "s", i, p.getInventory().getItem(i));
        }
        for (int k = 0; k < 5; k++) {
            thing(b, "w", k, Moves.worn(p, k));
        }
        for (int i = 0; i < swaps.size(); i++) {
            plan(b, p, i, swaps.get(i), reach);
        }
        return b.toString();
    }

    /** A thing's line: where, how many, how much of it is left, its kinds, id, name, enchantments, where it goes. */
    private static void thing(StringBuilder b, String tag, int at, ItemStack st) {
        if (st.isEmpty()) {
            return;
        }
        b.append(tag).append('\t').append(at).append('\t').append(st.getCount()).append('\t');
        b.append(st.isDamageableItem() && st.getMaxDamage() > 0 ? String.format(Locale.ROOT, "%.3f", Things.left(st)) : "-1").append('\t');
        boolean first = true;
        for (int i = 0; i < KINDS.length; i++) {
            if (Things.fits(KIND_PICKS[i], st, ItemStack.EMPTY, slotOf(KINDS[i]))) {
                if (!first) {
                    b.append(',');
                }
                b.append(KINDS[i]);
                first = false;
            }
        }
        b.append('\t').append(Things.id(st)).append('\t').append(clean(Things.name(st))).append('\t').append(Things.ench(st));
        // the places it may be put on, a bit each (the off hand takes anything)
        int places = 0;
        for (int k = 0; k < Moves.SLOTS.length; k++) {
            if (Things.goes(st, Moves.SLOTS[k])) {
                places |= 1 << k;
            }
        }
        b.append('\t').append(places).append('\n');
    }

    private static String clean(String s) {
        return s.replace('\t', ' ').replace('\n', ' ').replace('\r', ' ');
    }

    /** Where a kind is put on (armour on its own place, the rest in the hand). */
    private static EquipmentSlot slotOf(String kind) {
        return switch (kind) {
            case "helmet" -> EquipmentSlot.HEAD;
            case "chestplate", "elytra" -> EquipmentSlot.CHEST;
            case "leggings" -> EquipmentSlot.LEGS;
            case "boots" -> EquipmentSlot.FEET;
            default -> EquipmentSlot.OFFHAND;
        };
    }

    /** A swap's plan: the slot it would take now (-2: what it puts on is on), the slots with something of its list,
     *  "All armour"'s pieces. */
    private static void plan(StringBuilder b, LocalPlayer p, int i, AutoSwap.Swap s, Moves.Reach reach) {
        int from = -1;
        StringBuilder fits = new StringBuilder();
        StringBuilder pieces = new StringBuilder();
        if (s.place == AutoSwap.ALL) {
            for (int k = 1; k <= 4; k++) {
                int at = AutoSwap.piece(p, k, s.value, reach);
                if (at >= 0) {
                    pieces.append(pieces.isEmpty() ? "" : ",").append(k).append(':').append(at);
                }
            }
        } else if (s.place >= 0 && s.place < Moves.SLOTS.length && !s.picks.isEmpty()) {
            ItemStack worn = Moves.worn(p, s.place);
            from = switch (s.trigger) {
                case 0 -> AutoSwap.next(p, s, worn, reach);
                case 2 -> s.cond == 1 ? AutoSwap.better(p, s, worn, reach) : AutoSwap.first(p, s, worn, reach);
                default -> AutoSwap.first(p, s, worn, reach);
            };
            if (from < 0 && Things.fits(s.picks.get(0), worn, worn, Moves.SLOTS[s.place])) {
                from = -2;
            }
            for (int k = 0; k < 36; k++) {
                ItemStack st = p.getInventory().getItem(k);
                if (st.isEmpty() || !reach.takes(k, st)) {
                    continue;
                }
                for (Things.Pick pk : s.picks) {
                    if (Things.fits(pk, st, worn, Moves.SLOTS[s.place])) {
                        fits.append(fits.isEmpty() ? "" : ",").append(k);
                        break;
                    }
                }
            }
        }
        b.append("p\t").append(i).append('\t').append(from).append('\t').append(fits).append('\t').append(pieces).append('\n');
    }

    /** The editor's things, over the menu's picture (`scale` window px to GUI px). */
    public static void icons(GuiGraphicsExtractor graphics, Minecraft mc, float scale) {
        LocalPlayer p = mc.player;
        String all = p == null ? null : Native.guiCall("swapicons");
        if (all == null || all.isEmpty()) {
            return;
        }
        // the darkened ones' shade goes over every thing (a stratum over them): x, y, side, how dark, the part
        List<float[]> shades = new java.util.ArrayList<>();
        for (String line : all.split("\n")) {
            String[] f = line.split(" ", 10);
            if (f.length < 10) {
                continue;
            }
            float x, y, w, alpha, dim, c0x, c0y, c1x, c1y;
            try {
                x = Float.parseFloat(f[0]) / scale;
                y = Float.parseFloat(f[1]) / scale;
                w = Float.parseFloat(f[2]) / scale;
                alpha = Float.parseFloat(f[3]);
                dim = Float.parseFloat(f[4]);
                c0x = Float.parseFloat(f[5]) / scale;
                c0y = Float.parseFloat(f[6]) / scale;
                c1x = Float.parseFloat(f[7]) / scale;
                c1y = Float.parseFloat(f[8]) / scale;
            } catch (NumberFormatException e) {
                continue;
            }
            // drawn opaque: as their part fades they shrink into their middle instead
            float k = alpha >= 0.98F ? 1.0F : Math.max(0.0F, (alpha - 0.2F) / 0.78F);
            k = k * k * (3.0F - 2.0F * k);
            if (k < 0.05F) {
                continue;
            }
            x += w * (1.0F - k) / 2.0F;
            y += w * (1.0F - k) / 2.0F;
            w *= k;
            int[] part = {(int) Math.ceil(c0x), (int) Math.ceil(c0y), (int) Math.floor(c1x), (int) Math.floor(c1y)};
            if (part[2] <= part[0] || part[3] <= part[1] || x >= c1x || y >= c1y || x + w <= c0x || y + w <= c0y) {
                continue;
            }
            boolean whole = x >= c0x && y >= c0y && x + w <= c1x && y + w <= c1y;
            if (!whole) {
                graphics.enableScissor(part[0], part[1], part[2], part[3]);
            }
            draw(graphics, p, f[9].trim(), x, y, w);
            if (!whole) {
                graphics.disableScissor();
            }
            if (dim > 0.01F) {
                shades.add(new float[] {x, y, w, dim, part[0], part[1], part[2], part[3]});
            }
        }
        if (shades.isEmpty()) {
            return;
        }
        graphics.nextStratum();
        for (float[] s : shades) {
            graphics.enableScissor((int) s[4], (int) s[5], (int) s[6], (int) s[7]);
            var pose = graphics.pose();
            pose.pushMatrix();
            pose.translate(s[0], s[1]);
            pose.scale(s[2] / 16.0F, s[2] / 16.0F);
            graphics.fill(0, 0, 16, 16, net.minecraft.util.ARGB.color(Math.round(s[3] * 255.0F), 0x0b0b0d));
            pose.popMatrix();
            graphics.disableScissor();
        }
    }

    private static void draw(GuiGraphicsExtractor graphics, LocalPlayer p, String what, float x, float y, float w) {
        var pose = graphics.pose();
        if (what.length() == 2 && what.charAt(0) == 'e') {
            int k = what.charAt(1) - '0';
            if (k >= 0 && k < EMPTY.length) {
                pose.pushMatrix();
                pose.translate(x, y);
                pose.scale(w / 16.0F, w / 16.0F);
                graphics.blitSprite(RenderPipelines.GUI_TEXTURED, EMPTY[k], 0, 0, 16, 16, 0.35F);
                pose.popMatrix();
            }
            return;
        }
        ItemStack st = stack(p, what);
        if (st.isEmpty()) {
            return;
        }
        pose.pushMatrix();
        pose.translate(x, y);
        pose.scale(w / 16.0F, w / 16.0F);
        graphics.item(st, 0, 0, what.hashCode());
        pose.popMatrix();
    }

    /** Which thing: "s3" a slot's, "w0" what is worn, "k:totem" a kind's, "p2:0" a swap's (itself from the inventory
     *  when it is there, else one like it). */
    private static ItemStack stack(LocalPlayer p, String what) {
        try {
            switch (what.charAt(0)) {
                case 's' -> {
                    int i = Integer.parseInt(what.substring(1));
                    return i >= 0 && i < 36 ? p.getInventory().getItem(i) : ItemStack.EMPTY;
                }
                case 'w' -> {
                    int k = Integer.parseInt(what.substring(1));
                    return k >= 0 && k < Moves.SLOTS.length ? Moves.worn(p, k) : ItemStack.EMPTY;
                }
                case 'k' -> {
                    return shown(what.substring(2));
                }
                case 'p' -> {
                    int colon = what.indexOf(':');
                    int i = Integer.parseInt(what.substring(1, colon));
                    int n = Integer.parseInt(what.substring(colon + 1));
                    if (i < 0 || i >= swaps.size() || n < 0 || n >= swaps.get(i).picks.size()) {
                        return ItemStack.EMPTY;
                    }
                    Things.Pick pick = swaps.get(i).picks.get(n);
                    return pick.kind() != null ? shown(pick.kind()) : picked(p, pick);
                }
                default -> {
                    return ItemStack.EMPTY;
                }
            }
        } catch (RuntimeException e) {
            return ItemStack.EMPTY;
        }
    }

    /** A thing picked: itself where it is (a sphere's own face), else one of its kind of item. */
    private static ItemStack picked(LocalPlayer p, Things.Pick pick) {
        for (int i = 0; i < 36; i++) {
            ItemStack st = p.getInventory().getItem(i);
            if (!st.isEmpty() && Things.fits(pick, st, ItemStack.EMPTY, EquipmentSlot.OFFHAND)) {
                return st;
            }
        }
        for (int k = 0; k < Moves.SLOTS.length; k++) {
            ItemStack st = Moves.worn(p, k);
            if (!st.isEmpty() && Things.id(st).equals(pick.id()) && Things.name(st).equals(pick.name()) && Things.ench(st).equals(pick.ench())) {
                return st;
            }
        }
        return SHOWN.computeIfAbsent("id:" + pick.id(), key -> {
            Identifier id = Identifier.tryParse(pick.id());
            return id == null ? ItemStack.EMPTY : new ItemStack(BuiltInRegistries.ITEM.getValue(id));
        });
    }

    /** What shows a kind. */
    private static ItemStack shown(String kind) {
        return SHOWN.computeIfAbsent(kind, k -> {
            ItemStack st = new ItemStack(switch (k) {
                case "totem", "talisman" -> Items.TOTEM_OF_UNDYING;
                case "shield" -> Items.SHIELD;
                case "sphere" -> Items.PLAYER_HEAD;
                case "firework" -> Items.FIREWORK_ROCKET;
                case "golden_apple" -> Items.GOLDEN_APPLE;
                case "enchanted_apple" -> Items.ENCHANTED_GOLDEN_APPLE;
                case "food" -> Items.COOKED_BEEF;
                case "elytra" -> Items.ELYTRA;
                case "helmet" -> Items.NETHERITE_HELMET;
                case "chestplate" -> Items.NETHERITE_CHESTPLATE;
                case "leggings" -> Items.NETHERITE_LEGGINGS;
                case "boots" -> Items.NETHERITE_BOOTS;
                default -> Items.AIR;
            });
            if ("talisman".equals(k)) {
                st.set(DataComponents.ENCHANTMENT_GLINT_OVERRIDE, true);
            }
            return st;
        });
    }
}
