package cursed.recode.hud;

import com.mojang.blaze3d.platform.NativeImage;
import com.mojang.blaze3d.systems.RenderSystem;
import com.mojang.renderpearl.api.GpuFormat;
import com.mojang.renderpearl.api.textures.FilterMode;
import com.mojang.renderpearl.api.textures.GpuTexture;
import com.mojang.renderpearl.api.textures.GpuTextureView;
import cursed.recode.mixin.client.ContainerScreenAccess;
import cursed.recode.render.GuiFrame;
import cursed.recode.render.Native;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.Hud;
import net.minecraft.client.gui.components.ImageButton;
import net.minecraft.client.gui.components.Renderable;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.inventory.InventoryScreen;
import net.minecraft.client.model.HumanoidModel;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.client.renderer.RenderPipelines;
import net.minecraft.client.renderer.entity.state.ArmedEntityRenderState;
import net.minecraft.client.renderer.entity.state.EntityRenderState;
import net.minecraft.client.renderer.entity.state.HumanoidRenderState;
import net.minecraft.client.renderer.entity.state.LivingEntityRenderState;
import net.minecraft.client.renderer.item.ItemStackRenderState;
import net.minecraft.client.resources.model.sprite.Material;
import net.minecraft.core.Holder;
import net.minecraft.core.NonNullList;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.Style;
import net.minecraft.network.chat.TextColor;
import net.minecraft.util.ARGB;
import net.minecraft.util.RandomSource;
import net.minecraft.world.effect.MobEffect;
import net.minecraft.world.effect.MobEffectInstance;
import net.minecraft.world.entity.EquipmentSlot;
import net.minecraft.world.entity.HumanoidArm;
import net.minecraft.world.entity.Pose;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.entity.ai.attributes.AttributeModifier;
import net.minecraft.world.inventory.Slot;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemDisplayContext;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.component.ItemAttributeModifiers;
import net.minecraft.world.item.component.ItemContainerContents;
import net.minecraft.world.item.enchantment.Enchantment;
import net.minecraft.world.item.enchantment.ItemEnchantments;
import net.minecraft.world.food.FoodProperties;
import org.joml.Quaternionf;
import org.joml.Vector3f;
import org.jspecify.annotations.Nullable;

/**
 * Custom Inventory (Interface; native gui/hud/hudinv.rs): the survival inventory as the client's character screen. The
 * game's own screen and menu stay what they are (its slots, clicks, drags, the number keys, dropping, the packets),
 * but it is not drawn: this sends the native what the game knows each frame, puts its picture under everything, the
 * player's model where and as it says (the camera flying to a part, armour or a weapon tried on), the items in the
 * slots it laid out, the inspected item turning, then its picture over the items (counts, wear). The slot under the
 * pointer is the one under it in that layout. Anything going wrong: the game's own screen next frame.
 */
public final class HudInv {
    private HudInv() {
    }

    private static boolean on;
    private static boolean broken;
    private static @Nullable Screen shown;
    private static long openAt;
    private static final float[] OUT = new float[4096];
    private static int outN;
    private static float gs = 1.0F;
    private static @Nullable GpuTexture under;
    private static @Nullable GpuTextureView underView;
    private static @Nullable GpuTexture over;
    private static @Nullable GpuTextureView overView;
    private static final Map<Item, Integer> COLOURS = new HashMap<>();
    /** The effects' durations when first seen (their rings go from there). */
    private static final Map<Holder<MobEffect>, Integer> FIRST = new HashMap<>();
    private static final List<MobEffectInstance> EFFECTS = new ArrayList<>();
    private static final NonNullList<ItemStack> BOXED = NonNullList.withSize(27, ItemStack.EMPTY);
    private static ItemStack inspected = ItemStack.EMPTY;
    /** The inspected stack's tooltip lines as sent (made again only when the stack changes). */
    private static @Nullable ItemStack linesOf;
    private static int linesKey;
    private static String linesText = "";
    private static final RandomSource RANDOM = RandomSource.create(0L);

    /** From the HUD's state (Native.hudState: 79). */
    static void state(float[] s) {
        on = s[79] > 0.5F;
    }

    /** The client draws this screen (the survival inventory, the module on, our renderer up). */
    public static boolean active(@Nullable Screen screen) {
        Minecraft mc = Minecraft.getInstance();
        // (retired 15.10: the inventory is HudPack's blocks now; its helpers stay for the creative inventory)
        if (true || !on || broken || !(screen instanceof InventoryScreen) || !GuiFrame.active() || mc.player == null || mc.player.hasInfiniteMaterials()) {
            return false;
        }
        if (screen != shown) {
            shown = screen;
            openAt = System.nanoTime();
            FIRST.clear();
        }
        return true;
    }

    /** The screen was laid out: its recipe book's button goes (the client has its own crafting). */
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

    /** Instead of the game's background: the frame made, its picture under all, the model, the inspected item, the
     *  effects' icons. */
    public static void background(InventoryScreen screen, GuiGraphicsExtractor graphics, int mouseX, int mouseY, float partial) {
        try {
            frame(screen, graphics, mouseX, mouseY, partial);
        } catch (Throwable t) {
            broken = true;
            org.slf4j.LoggerFactory.getLogger("HudInv").error("Custom Inventory failed; the game's own inventory from now on", t);
        }
    }

    private static void frame(InventoryScreen screen, GuiGraphicsExtractor graphics, int mouseX, int mouseY, float partial) {
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
        StringBuilder text = new StringBuilder();
        double[] nums = facts(mc, p, screen, text);
        int id = Native.invFrame(w, h, nums, text.toString());
        outN = Native.invOut(OUT);
        int top = Native.invTop();
        if (id == 0 || outN < 2) {
            return;
        }
        pictures(graphics, w, h, id, top);
        int o = 2 + (int) OUT[1] * 10;
        Slot hov = hovered(mc);
        model(mc, p, graphics, OUT, o, gs, hov != null ? hov.getItem() : ItemStack.EMPTY);
        o += 11;
        if (OUT[o] > 0.5F && !inspected.isEmpty()) {
            cursed.recode.menus.ItemStage.inspect(graphics, inspected, OUT[o + 1], OUT[o + 2], OUT[o + 3], OUT[o + 4], OUT[o + 5], OUT[o + 6]);
        }
        o += 7;
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
    }

    /** The player's model in the stage's box, as the native's camera sees it (its 11 numbers at `o` in `out`); the
     *  hovered armour or weapon (`tried`) tried on when it says so. */
    static void model(Minecraft mc, LocalPlayer p, GuiGraphicsExtractor graphics, float[] out, int o, float gs, ItemStack tried) {
        int x0 = Math.round(out[o] / gs), y0 = Math.round(out[o + 1] / gs), x1 = Math.round(out[o + 2] / gs), y1 = Math.round(out[o + 3] / gs);
        if (x1 <= x0 || y1 <= y0) {
            return;
        }
        float scale = out[o + 4] / gs;
        float focus = out[o + 5];
        float yaw = out[o + 6], pitch = out[o + 7], headYaw = out[o + 8], headPitch = out[o + 9];
        int tryOn = Math.round(out[o + 10]);
        var renderer = mc.getEntityRenderDispatcher().getRenderer(p);
        EntityRenderState state = renderer.createRenderState(p, 1.0F);
        state.shadowPieces.clear();
        state.outlineColor = 0;
        if (state instanceof LivingEntityRenderState l) {
            l.bodyRot = 180.0F + yaw;
            l.yRot = headYaw;
            l.xRot = l.pose != Pose.FALL_FLYING ? headPitch : 0.0F;
            l.boundingBoxWidth = l.boundingBoxWidth / l.scale;
            l.boundingBoxHeight = l.boundingBoxHeight / l.scale;
            l.scale = 1.0F;
        }
        if (tryOn == 1 && !tried.isEmpty() && state instanceof HumanoidRenderState hs) {
            var eq = tried.get(DataComponents.EQUIPPABLE);
            if (eq != null) {
                switch (eq.slot()) {
                    case HEAD -> hs.headEquipment = tried;
                    case CHEST -> hs.chestEquipment = tried;
                    case LEGS -> hs.legsEquipment = tried;
                    case FEET -> hs.feetEquipment = tried;
                    default -> {
                    }
                }
            }
        } else if (tryOn == 2 && !tried.isEmpty() && state instanceof ArmedEntityRenderState as) {
            boolean right = as.mainArm == HumanoidArm.RIGHT;
            ItemDisplayContext ctx = right ? ItemDisplayContext.THIRD_PERSON_RIGHT_HAND : ItemDisplayContext.THIRD_PERSON_LEFT_HAND;
            if (right) {
                mc.getItemModelResolver().updateForLiving(as.rightHandItemState, tried, ctx, p);
                as.rightHandItemStack = tried;
                as.rightArmPose = HumanoidModel.ArmPose.ITEM;
            } else {
                mc.getItemModelResolver().updateForLiving(as.leftHandItemState, tried, ctx, p);
                as.leftHandItemStack = tried;
                as.leftArmPose = HumanoidModel.ArmPose.ITEM;
            }
        }
        Quaternionf tilt = new Quaternionf().rotateX(pitch * ((float) Math.PI / 180.0F));
        Quaternionf rot = new Quaternionf().rotateZ((float) Math.PI).mul(tilt);
        graphics.entity(state, scale, new Vector3f(0.0F, focus, 0.0F), rot, tilt, x0, y0, x1, y1);
    }

    /** Instead of the game's contents: the hovered slot set as the layout has it, the items in their places, the
     *  carried stack, the picture over all. */
    public static void contents(InventoryScreen screen, GuiGraphicsExtractor graphics, int mouseX, int mouseY, float partial) {
        try {
            items(screen, graphics, mouseX, mouseY);
        } catch (Throwable t) {
            broken = true;
            org.slf4j.LoggerFactory.getLogger("HudInv").error("Custom Inventory failed; the game's own inventory from now on", t);
        }
    }

    private static void items(InventoryScreen screen, GuiGraphicsExtractor graphics, int mouseX, int mouseY) {
        var acc = (ContainerScreenAccess) screen;
        acc.cursed$setHovered(slotAt(screen, mouseX, mouseY));
        if (outN < 2) {
            return;
        }
        var slots = screen.getMenu().slots;
        int r = (int) OUT[1];
        for (int i = 0; i < r; i++) {
            int o = 2 + i * 10;
            int idx = (int) OUT[o];
            if (idx < 0 || idx >= slots.size() || OUT[o + 5] < 0.02F) {
                continue;
            }
            ItemStack s = slots.get(idx).getItem();
            if (s.isEmpty()) {
                continue;
            }
            item(graphics, s, OUT[o + 1] / gs, OUT[o + 2] / gs, OUT[o + 3] / gs, OUT[o + 4], idx);
        }
        // a box's contents in the inspection
        int o = 2 + r * 10 + 11 + 7;
        int e = (int) OUT[o];
        o += 1 + e * 5;
        int k = (int) OUT[o++];
        for (int i = 0; i < k; i++, o += 4) {
            int b = (int) OUT[o];
            if (b >= 0 && b < BOXED.size() && !BOXED.get(b).isEmpty()) {
                item(graphics, BOXED.get(b), OUT[o + 1] / gs, OUT[o + 2] / gs, OUT[o + 3] / gs, 1.0F, b);
            }
        }
        float side = OUT[o] / gs;
        ItemStack carried = screen.getMenu().getCarried();
        graphics.nextStratum();
        if (!carried.isEmpty()) {
            item(graphics, carried, mouseX - side / 2.0F, mouseY - side / 2.0F, side, 1.08F, 0);
        }
        graphics.nextStratum();
        over(graphics);
    }

    /** The frame's two pictures (the one under the items: `id`, the one over them: `top`) sent into their textures
     *  (made for the window's size), the one under drawn over the whole GUI. The creative inventory's too. */
    static void pictures(GuiGraphicsExtractor graphics, int w, int h, int id, int top) {
        if (under == null || under.getWidth(0) != w || under.getHeight(0) != h) {
            release();
            var dev = RenderSystem.getDevice();
            under = dev.createTexture("Cursed inventory", GpuTexture.USAGE_COPY_DST | GpuTexture.USAGE_TEXTURE_BINDING, GpuFormat.RGBA8_UNORM, w, h, 1, 1);
            underView = dev.createTextureView(under);
            over = dev.createTexture("Cursed inventory over", GpuTexture.USAGE_COPY_DST | GpuTexture.USAGE_TEXTURE_BINDING, GpuFormat.RGBA8_UNORM, w, h, 1, 1);
            overView = dev.createTextureView(over);
        }
        GuiFrame.sendExtra(0, under, id);
        GuiFrame.sendExtra(1, over, top);
        graphics.blit(underView, RenderSystem.getSamplerCache().getClampToEdge(FilterMode.NEAREST), 0, 0, graphics.guiWidth(), graphics.guiHeight(), 0.0F, 1.0F, 0.0F, 1.0F);
    }

    /** The picture over the items (their counts and wear), over the whole GUI. */
    static void over(GuiGraphicsExtractor graphics) {
        if (overView != null) {
            graphics.blit(overView, RenderSystem.getSamplerCache().getClampToEdge(FilterMode.NEAREST), 0, 0, graphics.guiWidth(), graphics.guiHeight(), 0.0F, 1.0F, 0.0F, 1.0F);
        }
    }

    static void item(GuiGraphicsExtractor graphics, ItemStack stack, float x, float y, float size, float lift, int seed) {
        var pose = graphics.pose();
        pose.pushMatrix();
        pose.translate(x + size / 2.0F, y + size / 2.0F);
        float k = size * lift / 16.0F;
        pose.scale(k, k);
        pose.translate(-8.0F, -8.0F);
        graphics.item(stack, 0, 0, seed);
        pose.popMatrix();
    }

    /** The slot under (x, y) (GUI units) in the layout of the last frame. */
    public static @Nullable Slot slotAt(net.minecraft.client.gui.screens.inventory.AbstractContainerScreen<?> screen, double x, double y) {
        float wx = (float) (x * gs), wy = (float) (y * gs);
        var slots = screen.getMenu().slots;
        int r = outN >= 2 ? (int) OUT[1] : 0;
        for (int i = 0; i < r; i++) {
            int o = 2 + i * 10;
            if (wx >= OUT[o + 6] && wx < OUT[o + 8] && wy >= OUT[o + 7] && wy < OUT[o + 9]) {
                int idx = (int) OUT[o];
                return idx >= 0 && idx < slots.size() && slots.get(idx).isActive() ? slots.get(idx) : null;
            }
        }
        return null;
    }

    /** Whether (x, y) (GUI units) is outside the screen for the game (a stack let go there is thrown): the stage in the
     *  middle, where the player stands, and what is beyond the columns. */
    public static boolean outside(double x, double y) {
        if (outN < 2) {
            return false;
        }
        float wx = (float) (x * gs), wy = (float) (y * gs);
        int o = 2 + (int) OUT[1] * 10;
        boolean stage = wx >= OUT[o] && wx < OUT[o + 2] && wy >= OUT[o + 1] && wy < OUT[o + 3];
        var win = Minecraft.getInstance().getWindow();
        float u = Math.min(win.getWidth() / 1920.0F, win.getHeight() / 1080.0F);
        float ox = (win.getWidth() - 1920.0F * u) / 2.0F;
        return stage || wx < ox + 60.0F * u || wx > ox + 1860.0F * u;
    }

    private static @Nullable Slot hovered(Minecraft mc) {
        if (mc.gui.screen() instanceof InventoryScreen s) {
            return ((ContainerScreenAccess) s).cursed$hovered();
        }
        return null;
    }

    // ------------------------------------------------------------------------------------------ what the game knows

    private static double[] facts(Minecraft mc, LocalPlayer p, InventoryScreen screen, StringBuilder text) {
        List<Double> n = new ArrayList<>(700);
        var menu = screen.getMenu();
        var acc = (ContainerScreenAccess) screen;
        float since = (System.nanoTime() - openAt) / 1e9F;
        n.add((double) gs);
        n.add((double) since);
        n.add(mc.mouseHandler.xpos());
        n.add(mc.mouseHandler.ypos());
        n.add(mc.mouseHandler.isLeftPressed() ? 1.0 : 0.0);
        n.add((double) p.getInventory().getSelectedSlot());
        ItemStack carried = menu.getCarried();
        n.add((double) carried.getCount());
        n.add((double) p.getHealth());
        n.add((double) p.getMaxHealth());
        n.add((double) p.getAbsorptionAmount());
        n.add((double) p.getArmorValue());
        n.add(p.getAttributeValue(Attributes.ARMOR_TOUGHNESS));
        n.add((double) p.experienceLevel);
        n.add((double) p.experienceProgress);
        n.add((double) p.getXpNeededForNextLevel());
        n.add(0.0);
        var slots = menu.slots;
        boolean quick = acc.cursed$quick();
        var quickSlots = acc.cursed$quickSlots();
        n.add((double) slots.size());
        for (Slot sl : slots) {
            slot(n, mc, sl.getItem(), quick && quickSlots.contains(sl));
        }
        // effects: their names in the text
        EFFECTS.clear();
        EFFECTS.addAll(p.getActiveEffects());
        EFFECTS.sort((a, b) -> Integer.compare(b.getDuration(), a.getDuration()));
        if (EFFECTS.size() > 6) {
            EFFECTS.subList(6, EFFECTS.size()).clear();
        }
        n.add((double) EFFECTS.size());
        for (MobEffectInstance e : EFFECTS) {
            int d = e.isInfiniteDuration() ? -1 : e.getDuration();
            int first = FIRST.merge(e.getEffect(), Math.max(d, 1), Math::max);
            n.add((double) d);
            n.add((double) e.getAmplifier());
            n.add((double) first);
        }
        // the inspected item: the one under the pointer, else the one in the hand
        Slot hov = acc.cursed$hovered();
        ItemStack ins = hov != null && hov.hasItem() ? hov.getItem() : p.getInventory().getSelectedItem();
        inspected = carried.isEmpty() ? ins : ItemStack.EMPTY;
        StringBuilder words = new StringBuilder();
        inspected(n, words, mc, inspected, BOXED);
        // the words
        text.append("name\t").append(p.getName().getString()).append('\n');
        int[] eq = {5, 6, 7, 8, 45};
        for (int k = 0; k < eq.length; k++) {
            if (eq[k] >= slots.size()) {
                continue;
            }
            ItemStack s = slots.get(eq[k]).getItem();
            text.append("eq\t").append(k).append('\t').append(clean(s.isEmpty() ? "" : s.getHoverName().getString())).append('\t').append(clean(enchantments(s))).append('\n');
        }
        for (MobEffectInstance e : EFFECTS) {
            text.append("fx\t").append(clean(e.getEffect().value().getDisplayName().getString())).append('\n');
        }
        text.append(words);
        double[] out = new double[n.size()];
        for (int i = 0; i < out.length; i++) {
            out[i] = n.get(i);
        }
        return out;
    }

    /** A slot's 10 numbers (native hudinv read_slot): count, wear, its most, name's colour, picture's colour, kind,
     *  glint, in the drag, which item. */
    static void slot(List<Double> n, Minecraft mc, ItemStack s, boolean drag) {
        n.add((double) s.getCount());
        n.add(s.isDamageableItem() ? (double) s.getDamageValue() : 0.0);
        n.add(s.isDamageableItem() ? (double) s.getMaxDamage() : 0.0);
        n.add(s.isEmpty() ? 0.0 : (double) nameColour(s));
        n.add(s.isEmpty() ? 0.0 : (double) colour(mc, s));
        n.add((double) kind(s));
        n.add(s.hasFoil() ? 1.0 : 0.0);
        n.add(drag ? 1.0 : 0.0);
        n.add(s.isEmpty() ? 0.0 : (double) (BuiltInRegistries.ITEM.getId(s.getItem()) + 1));
        n.add(0.0);
    }

    /** The inspected item's numbers (native hudinv read_insp; none: a 0) and its words in `text` ("in", "il": the
     *  game's tooltip lines, made again only when the stack changes); a box's contents into `boxed`. */
    static void inspected(List<Double> n, StringBuilder text, Minecraft mc, ItemStack ins, NonNullList<ItemStack> boxed) {
        if (ins.isEmpty()) {
            n.add(0.0);
            return;
        }
        n.add(1.0);
        n.add((double) nameColour(ins));
        n.add((double) ins.getCount());
        n.add(ins.isDamageableItem() ? (double) ins.getDamageValue() : 0.0);
        n.add(ins.isDamageableItem() ? (double) ins.getMaxDamage() : 0.0);
        for (double v : stats(ins)) {
            n.add(v);
        }
        ItemContainerContents box = ins.get(DataComponents.CONTAINER);
        if (box != null && box.size() > 0 && box.size() <= 27) {
            n.add(1.0);
            for (int i = 0; i < 27; i++) {
                boxed.set(i, ItemStack.EMPTY);
            }
            box.copyInto(boxed);
            for (int i = 0; i < 27; i++) {
                n.add((double) boxed.get(i).getCount());
            }
        } else {
            n.add(0.0);
        }
        text.append("in\t").append(clean(ins.getHoverName().getString())).append('\t').append(kindName(ins)).append('\n');
        int key = ins.getCount() * 31 + ins.getDamageValue();
        if (linesOf != ins || linesKey != key) {
            StringBuilder b = new StringBuilder();
            List<Component> lines = Screen.getTooltipFromItem(mc, ins);
            for (int i = 1; i < lines.size() && i < 14; i++) {
                Component line = lines.get(i);
                b.append("il\t").append(String.format("%06X", firstColour(line, 0xA3A3A3))).append('\t').append(clean(line.getString())).append('\n');
            }
            linesOf = ins;
            linesKey = key;
            linesText = b.toString();
        }
        text.append(linesText);
    }

    static String clean(String s) {
        return s.replace('\t', ' ').replace('\n', ' ');
    }

    /** 1..4 armour that can be worn (head, chest, legs, feet), 5 held (weapons, tools and the like), 6 other, 0 none. */
    static int kind(ItemStack s) {
        if (s.isEmpty()) {
            return 0;
        }
        var eq = s.get(DataComponents.EQUIPPABLE);
        if (eq != null) {
            EquipmentSlot slot = eq.slot();
            if (slot == EquipmentSlot.HEAD) {
                return 1;
            }
            if (slot == EquipmentSlot.CHEST) {
                return 2;
            }
            if (slot == EquipmentSlot.LEGS) {
                return 3;
            }
            if (slot == EquipmentSlot.FEET) {
                return 4;
            }
        }
        if (s.has(DataComponents.WEAPON) || s.has(DataComponents.TOOL) || s.is(Items.BOW) || s.is(Items.CROSSBOW) || s.is(Items.TRIDENT) || s.is(Items.MACE)
                || s.is(Items.SHIELD) || s.is(Items.TOTEM_OF_UNDYING) || s.is(Items.FISHING_ROD) || s.is(Items.ENDER_PEARL) || s.is(Items.FIREWORK_ROCKET)) {
            return 5;
        }
        return 6;
    }

    static String kindName(ItemStack s) {
        int k = kind(s);
        boolean r = !Minecraft.getInstance().options.languageCode.startsWith("en");
        if (k >= 1 && k <= 4) {
            return r ? "Броня" : "Armour";
        }
        if (s.has(DataComponents.WEAPON)) {
            return r ? "Оружие" : "Weapon";
        }
        if (s.has(DataComponents.TOOL)) {
            return r ? "Инструмент" : "Tool";
        }
        if (s.has(DataComponents.FOOD)) {
            return r ? "Еда" : "Food";
        }
        return s.getItem() instanceof net.minecraft.world.item.BlockItem ? (r ? "Блок" : "Block") : (r ? "Предмет" : "Item");
    }

    /** Its enchantments in a line ("Sharpness V · Mending"). */
    private static String enchantments(ItemStack s) {
        if (s.isEmpty()) {
            return "";
        }
        ItemEnchantments en = s.getEnchantments();
        StringBuilder b = new StringBuilder();
        for (var e : en.entrySet()) {
            if (!b.isEmpty()) {
                b.append(" · ");
            }
            b.append(Enchantment.getFullname(e.getKey(), e.getIntValue()).getString());
        }
        return b.toString();
    }

    /** Damage, attack speed, armour, toughness, knockback resistance, food, saturation (-1: none). */
    static double[] stats(ItemStack s) {
        double[] v = {-1, -1, -1, -1, -1, -1, -1};
        ItemAttributeModifiers mods = s.get(DataComponents.ATTRIBUTE_MODIFIERS);
        if (mods != null) {
            double dmg = 0, spd = 0, arm = 0, tough = 0, kb = 0;
            boolean hasDmg = false, hasSpd = false, hasArm = false, hasTough = false, hasKb = false;
            for (ItemAttributeModifiers.Entry e : mods.modifiers()) {
                if (e.modifier().operation() != AttributeModifier.Operation.ADD_VALUE) {
                    continue;
                }
                double a = e.modifier().amount();
                if (e.attribute().equals(Attributes.ATTACK_DAMAGE)) {
                    dmg += a;
                    hasDmg = true;
                } else if (e.attribute().equals(Attributes.ATTACK_SPEED)) {
                    spd += a;
                    hasSpd = true;
                } else if (e.attribute().equals(Attributes.ARMOR)) {
                    arm += a;
                    hasArm = true;
                } else if (e.attribute().equals(Attributes.ARMOR_TOUGHNESS)) {
                    tough += a;
                    hasTough = true;
                } else if (e.attribute().equals(Attributes.KNOCKBACK_RESISTANCE)) {
                    kb += a;
                    hasKb = true;
                }
            }
            if (hasDmg) {
                v[0] = 1.0 + dmg;
            }
            if (hasSpd) {
                v[1] = 4.0 + spd;
            }
            if (hasArm) {
                v[2] = arm;
            }
            if (hasTough && tough > 0) {
                v[3] = tough;
            }
            if (hasKb && kb > 0) {
                v[4] = kb;
            }
        }
        FoodProperties food = s.get(DataComponents.FOOD);
        if (food != null) {
            v[5] = food.nutrition();
            v[6] = food.saturation();
        }
        return v;
    }

    /** The name's colour: its own (a server's), else the rarity's. */
    static int nameColour(ItemStack s) {
        int c = firstColour(s.getStyledHoverName(), -1);
        if (c >= 0) {
            return c;
        }
        return switch (s.getRarity()) {
            case UNCOMMON -> 0xFFFF55;
            case RARE -> 0x55FFFF;
            case EPIC -> 0xFF55FF;
            default -> 0xECECEC;
        };
    }

    /** The first colour a component's text is written in (`fallback` if none). */
    static int firstColour(Component c, int fallback) {
        int[] found = {fallback};
        boolean[] done = {false};
        // (a colour written into the text itself, § and a letter, counts as the game draws it)
        HudText.pieces(c, (style, text) -> {
            TextColor col = style.getColor();
            if (!done[0] && col != null) {
                found[0] = col.getValue() & 0xFFFFFF;
                done[0] = true;
            }
        });
        return found[0];
    }

    /** An item's own colour: its picture's pixels, the saturated ones counting more (made once an item). */
    static int colour(Minecraft mc, ItemStack s) {
        Item item = s.getItem();
        Integer known = COLOURS.get(item);
        if (known != null) {
            return known;
        }
        int v = 0xC8C8D2;
        try {
            ItemStackRenderState st = new ItemStackRenderState();
            mc.getItemModelResolver().updateForTopItem(st, s, ItemDisplayContext.GUI, mc.level, null, 0);
            Material.Baked m = st.pickParticleMaterial(RANDOM);
            if (m != null) {
                NativeImage im = m.sprite().contents().originalImage;
                double r = 0, g = 0, b = 0, wsum = 0;
                int w = im.getWidth(), h = Math.min(im.getHeight(), im.getWidth());
                for (int y = 0; y < h; y++) {
                    for (int x = 0; x < w; x++) {
                        int px = im.getPixel(x, y);
                        if ((px >>> 24) < 128) {
                            continue;
                        }
                        int pr = px >> 16 & 255, pg = px >> 8 & 255, pb = px & 255;
                        int mx = Math.max(pr, Math.max(pg, pb)), mn = Math.min(pr, Math.min(pg, pb));
                        double k = 0.15 + (mx - mn) / 255.0 * 2.0 + mx / 255.0 * 0.3;
                        r += pr * k;
                        g += pg * k;
                        b += pb * k;
                        wsum += k;
                    }
                }
                if (wsum > 0) {
                    v = (int) Math.round(r / wsum) << 16 | (int) Math.round(g / wsum) << 8 | (int) Math.round(b / wsum);
                }
            }
        } catch (Throwable ignored) {
            // the neutral light
        }
        COLOURS.put(item, v);
        return v;
    }

    private static void release() {
        if (underView != null) {
            underView.close();
        }
        if (under != null) {
            under.close();
        }
        if (overView != null) {
            overView.close();
        }
        if (over != null) {
            over.close();
        }
        under = null;
        underView = null;
        over = null;
        overView = null;
    }
}
