package cursed.recode.fx;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.screens.inventory.tooltip.ClientTooltipComponent;
import net.minecraft.core.BlockPos;
import net.minecraft.core.component.DataComponents;
import net.minecraft.world.inventory.tooltip.TooltipComponent;
import net.minecraft.world.item.BlockItem;
import net.minecraft.world.item.DyeColor;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.component.ItemContainerContents;
import net.minecraft.world.level.block.ShulkerBoxBlock;
import org.jspecify.annotations.Nullable;

/**
 * Shulker Preview (the menu's module "shulker-preview"): what a shulker box holds, seen without opening it. In a
 * tooltip a grid in the box's colour under its name (GuiGraphicsExtractorMixin, ItemMixin, ItemContainerContentsMixin);
 * the ender chest as it was last seen open; a placed box as it was last seen open (its place known by the click that
 * opened it). Over the world (the box looked at on the ground, in a frame, placed) the HUD draws it (native gui/hud).
 */
public final class ShulkerPreview {
    private static Map<String, String> set = Map.of();
    private static long polled;
    /** The ender chest as last seen; placed boxes as last seen (by server, world and place). */
    private static List<ItemStack> ender = List.of();
    private static String enderScope = "";
    private static final Map<String, List<ItemStack>> PLACED = new HashMap<>();
    /** The block a container was last opened by (its click). */
    private static @Nullable BlockPos clicked;

    private ShulkerPreview() {
    }

    static Map<String, String> settings() {
        long now = System.nanoTime();
        if (now - polled > 250_000_000L) {
            polled = now;
            set = Fx.settings("shulker-preview");
        }
        return set;
    }

    public static boolean on(String label) {
        Map<String, String> s = settings();
        return "1".equals(s.get("on")) && "1".equals(s.get(label));
    }

    private static boolean totals() {
        return "Totals".equals(settings().get("Style"));
    }

    /** The box's colour (none: the plain box's purple-grey). */
    static int tint(ItemStack stack) {
        if (!"1".equals(settings().get("Box colour"))) {
            return 0x8a7a92;
        }
        if (stack.getItem() instanceof BlockItem bi && bi.getBlock() instanceof ShulkerBoxBlock sb) {
            DyeColor c = sb.getColor();
            return c == null ? 0x8a6c96 : c.getTextureDiffuseColor() & 0xffffff;
        }
        return 0x1f5248;
    }

    static boolean isBox(ItemStack stack) {
        return stack.getItem() instanceof BlockItem bi && bi.getBlock() instanceof ShulkerBoxBlock;
    }

    /** What a box (or the ender chest) holds, by slot (empty slots kept). */
    static @Nullable List<ItemStack> contents(ItemStack stack) {
        if (isBox(stack)) {
            ItemContainerContents c = stack.getOrDefault(DataComponents.CONTAINER, ItemContainerContents.EMPTY);
            List<ItemStack> l = new ArrayList<>(c.itemCopies().toList());
            while (l.size() < 27) {
                l.add(ItemStack.EMPTY);
            }
            return l;
        }
        if (stack.is(Items.ENDER_CHEST) && on("Ender chest") && !ender.isEmpty() && enderScope.equals(scope(Minecraft.getInstance()))) {
            return ender;
        }
        return null;
    }

    /** The tooltip's image for a stack (ItemMixin): a box's grid, while the inventory's preview is on. */
    public static Optional<TooltipComponent> image(ItemStack stack) {
        if (!on("In the inventory")) {
            return Optional.empty();
        }
        List<ItemStack> l = contents(stack);
        return l == null ? Optional.empty() : Optional.of(new Image(l, tint(stack)));
    }

    /** Whether the game's own lines of what it holds give way to the grid (ItemContainerContentsMixin). */
    public static boolean replacesLines() {
        return on("In the inventory");
    }

    /** As the list is shown: every slot (Grid) or each kind once with all of it (Totals), the most first. */
    static List<ItemStack> shown(List<ItemStack> l) {
        if (!totals()) {
            return l;
        }
        List<ItemStack> out = new ArrayList<>();
        for (ItemStack s : l) {
            if (s.isEmpty()) {
                continue;
            }
            ItemStack same = out.stream().filter(o -> ItemStack.isSameItemSameComponents(o, s)).findFirst().orElse(null);
            if (same != null) {
                same.setCount(same.getCount() + s.getCount());
            } else {
                out.add(s.copy());
            }
        }
        out.sort((a, b) -> Integer.compare(b.getCount(), a.getCount()));
        return out;
    }

    /** A container's click (MultiPlayerGameModeMixin): which block it was. */
    public static void used(BlockPos pos) {
        clicked = pos;
    }

    /** Each tick: an open ender chest or shulker box remembered as it is (Minecraft's tick). */
    public static void tick(Minecraft mc) {
        var screen = mc.gui.screen();
        if (mc.player == null || screen == null) {
            return;
        }
        var menu = mc.player.containerMenu;
        if (screen instanceof net.minecraft.client.gui.screens.inventory.ShulkerBoxScreen && clicked != null) {
            List<ItemStack> l = new ArrayList<>();
            for (int i = 0; i < 27 && i < menu.slots.size(); i++) {
                l.add(menu.slots.get(i).getItem().copy());
            }
            PLACED.put(key(mc, clicked), l);
        } else if (screen instanceof net.minecraft.client.gui.screens.inventory.ContainerScreen cs
                && cs.getTitle().getContents() instanceof net.minecraft.network.chat.contents.TranslatableContents t && "container.enderchest".equals(t.getKey())) {
            List<ItemStack> l = new ArrayList<>();
            for (int i = 0; i < 27 && i < menu.slots.size(); i++) {
                l.add(menu.slots.get(i).getItem().copy());
            }
            ender = l;
            enderScope = scope(mc);
        }
    }

    private static String scope(Minecraft mc) {
        var server = mc.getCurrentServer();
        return (server == null ? "local:" + System.identityHashCode(mc.getSingleplayerServer()) : server.ip.toLowerCase(java.util.Locale.ROOT))
                + "|" + (mc.player == null ? "" : mc.player.getUUID());
    }

    private static String itemFacts(List<ItemStack> items) {
        StringBuilder b = new StringBuilder();
        for (int i = 0; i < items.size(); i++) {
            if (i > 0) b.append('\u001d');
            ItemStack item = items.get(i);
            if (!item.isEmpty()) b.append(net.minecraft.core.registries.BuiltInRegistries.ITEM.getKey(item.getItem()))
                    .append(' ').append(item.getHoverName().getString().replace('\t', ' ').replace('\t', ' ').replace('\u001d', ' ').replace('\u001e', ' ').replace('\u001f', ' '));
        }
        return b.toString();
    }

    private static String key(Minecraft mc, BlockPos pos) {
        var server = mc.getCurrentServer();
        return scope(mc) + "|" + mc.level.dimension().identifier() + "|" + pos.asLong();
    }

    /** A placed box as last seen open (none: never opened, or not here). */
    static @Nullable List<ItemStack> placed(Minecraft mc, BlockPos pos) {
        return mc.level == null ? null : PLACED.get(key(mc, pos));
    }

    // ---------------------------------------------------------------- over the world (the HUD draws them: gui/hudshulker.rs)
    /** The items of each panel told this frame (the HUD's icons, kind 11: panel * 64 + slot). */
    private static final ItemStack[][] PANELS = new ItemStack[4][];

    public static ItemStack icon(int index) {
        int p = index / 64, i = index % 64;
        ItemStack[] a = p >= 0 && p < PANELS.length ? PANELS[p] : null;
        return a != null && i < a.length && a[i] != null ? a[i] : ItemStack.EMPTY;
    }

    /** Where a point of the world is on the screen (window px) and how far: none when behind. */
    private static float @Nullable [] project(Minecraft mc, net.minecraft.world.phys.Vec3 p, int w, int h) {
        var cam = mc.gameRenderer.mainCamera();
        var c = cam.position();
        org.joml.Vector3f v = new org.joml.Vector3f((float) (p.x - c.x), (float) (p.y - c.y), (float) (p.z - c.z));
        v.rotate(new org.joml.Quaternionf(cam.rotation()).conjugate());
        org.joml.Vector4f clip = new org.joml.Vector4f(v, 1.0F).mul(cursed.recode.render.NativeTerrain.PROJECTION);
        if (clip.w < 0.05F) {
            return null;
        }
        return new float[] {(clip.x / clip.w * 0.5F + 0.5F) * w, (1.0F - (clip.y / clip.w * 0.5F + 0.5F)) * h, (float) Math.sqrt(v.lengthSquared())};
    }

    private static int panel(StringBuilder b, int n, String id, List<ItemStack> l, ItemStack box, float @Nullable [] at, boolean hand) {
        if (n >= PANELS.length) {
            return n;
        }
        List<ItemStack> shown = shown(l);
        PANELS[n] = shown.toArray(new ItemStack[0]);
        StringBuilder counts = new StringBuilder();
        for (ItemStack s : shown) {
            if (!counts.isEmpty()) {
                counts.append(' ');
            }
            counts.append(s.isEmpty() ? 0 : s.getCount());
        }
        float scale = at == null ? 1.0F : Math.max(0.6F, Math.min(1.0F, 1.15F - at[2] / 18.0F));
        String name = box.has(DataComponents.CUSTOM_NAME) ? box.getHoverName().getString().replace('\u001f', ' ').replace('\u001e', ' ') : "";
        if (!b.isEmpty()) {
            b.append('\u001e');
        }
        b.append(id).append('\u001f').append(n).append('\u001f').append(at == null ? 0 : at[0]).append('\u001f').append(at == null ? 0 : at[1])
                .append('\u001f').append(scale).append('\u001f').append(String.format("%06x", tint(box))).append('\u001f').append(hand ? "1" : "0")
                .append('\u001f').append(name).append('\u001f').append(counts).append('\u001f').append(itemFacts(shown));
        return n + 1;
    }

    /** Each frame of the HUD (HudLayer, before it is drawn): the boxes to show over the world and in the hand. */
    public static void frame(Minecraft mc, float partial, int w, int h) {
        StringBuilder b = new StringBuilder();
        int n = 0;
        var p = mc.player;
        if (p != null && mc.level != null && "1".equals(settings().get("on")) && mc.gui.screen() == null) {
            ItemStack held = p.getMainHandItem();
            if (on("In your hand") && isBox(held)) {
                n = panel(b, n, "hand", contents(held), held, null, true);
            }
            double reach;
            try {
                reach = Double.parseDouble(settings().getOrDefault("Distance", "8"));
            } catch (NumberFormatException e) {
                reach = 8.0;
            }
            var eye = p.getEyePosition(partial);
            var look = p.getViewVector(partial);
            // on the ground or in a frame: the one looked at most nearly
            if (on("On the ground")) {
                net.minecraft.world.entity.Entity best = null;
                ItemStack bestStack = ItemStack.EMPTY;
                double bestDot = Math.cos(Math.toRadians(9.0));
                for (net.minecraft.world.entity.Entity e : mc.level.entitiesForRendering()) {
                    ItemStack s = e instanceof net.minecraft.world.entity.item.ItemEntity ie ? ie.getItem()
                            : e instanceof net.minecraft.world.entity.decoration.ItemFrame f ? f.getItem() : ItemStack.EMPTY;
                    if (!isBox(s)) {
                        continue;
                    }
                    var at = e.getPosition(partial).add(0.0, e.getBbHeight() * 0.5, 0.0);
                    double d = at.distanceTo(eye);
                    double dot = at.subtract(eye).normalize().dot(look);
                    if (d <= reach && dot > bestDot) {
                        best = e;
                        bestStack = s;
                        bestDot = dot;
                    }
                }
                if (best != null) {
                    var top = best.getPosition(partial).add(0.0, best.getBbHeight() + 0.35, 0.0);
                    float[] at = project(mc, top, w, h);
                    if (at != null) {
                        n = panel(b, n, "e" + best.getId(), contents(bestStack), bestStack, at, false);
                    }
                }
            }
            // a placed one looked at, as it was seen open
            if (on("Placed boxes") && mc.hitResult instanceof net.minecraft.world.phys.BlockHitResult bh
                    && bh.getType() == net.minecraft.world.phys.HitResult.Type.BLOCK) {
                BlockPos pos = bh.getBlockPos();
                var state = mc.level.getBlockState(pos);
                List<ItemStack> l = placed(mc, pos);
                if (state.getBlock() instanceof ShulkerBoxBlock sb && l != null) {
                    var top = net.minecraft.world.phys.Vec3.atCenterOf(pos).add(0.0, 0.85, 0.0);
                    float[] at = project(mc, top, w, h);
                    if (at != null && top.distanceTo(eye) <= reach + 1.0) {
                        n = panel(b, n, "b" + pos.asLong(), l, new ItemStack(sb.asItem()), at, false);
                    }
                }
            }
        }
        for (int i = n; i < PANELS.length; i++) {
            PANELS[i] = null;
        }
        cursed.recode.render.Native.guiCall("shulkers\t" + b);
    }

    /** The tooltip's image: what is shown and its colour. */
    public record Image(List<ItemStack> items, int tint) implements TooltipComponent {
    }

    /** The grid drawn in a tooltip: nine across, as many rows as it needs; each slot a little cell, the box's colour
     *  round it all. */
    public static final class Client implements ClientTooltipComponent {
        private static final int CELL = 18;
        private final List<ItemStack> items;
        private final int tint;

        public Client(Image image) {
            this.items = shown(image.items());
            this.tint = image.tint();
        }

        private int rows() {
            return Math.max(1, (this.items.size() + 8) / 9);
        }

        @Override
        public int getHeight(Font font) {
            return this.rows() * CELL + 6;
        }

        @Override
        public int getWidth(Font font) {
            return 9 * CELL + 4;
        }

        @Override
        public void extractImage(Font font, int x, int y, int w, int h, GuiGraphicsExtractor graphics) {
            int rows = this.rows();
            int x1 = x + 9 * CELL + 2, y1 = y + rows * CELL + 2;
            // the box's colour round it, a soft fill of it behind the cells
            graphics.fill(x - 1, y - 1, x1 + 1, y1 + 1, 0x55000000 | this.tint);
            graphics.fill(x, y, x1, y1, 0xc8101012);
            graphics.fill(x, y, x1, y1, 0x1c000000 | this.tint);
            String found = cursed.recode.render.Native.guiCall("shulk-filter\t" + itemFacts(this.items));
            boolean searching = !settings().getOrDefault("Find item", "").isBlank();
            for (int i = 0; i < rows * 9; i++) {
                int cx = x + 1 + (i % 9) * CELL, cy = y + 1 + (i / 9) * CELL;
                boolean match = i < found.length() && found.charAt(i) == '1';
                graphics.fill(cx + 1, cy + 1, cx + CELL - 1, cy + CELL - 1, searching && match ? 0x88ffffff : 0x22ffffff);
                if (i < this.items.size()) {
                    ItemStack s = this.items.get(i);
                    if (!s.isEmpty()) {
                        graphics.item(s, cx + 1, cy + 1);
                        graphics.itemDecorations(font, s, cx + 1, cy + 1, s.getCount() > 99 ? compact(s.getCount()) : null);
                        if (searching && !match) graphics.fill(cx + 1, cy + 1, cx + CELL - 1, cy + CELL - 1, 0xb0101012);
                    }
                }
            }
        }

        private static String compact(int n) {
            return n >= 1000 ? (n / 1000) + "k" : String.valueOf(n);
        }
    }
}
