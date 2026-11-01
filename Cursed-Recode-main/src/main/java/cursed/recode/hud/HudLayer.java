package cursed.recode.hud;

import com.mojang.blaze3d.platform.InputConstants;
import com.mojang.blaze3d.systems.RenderSystem;
import com.mojang.renderpearl.api.GpuFormat;
import com.mojang.renderpearl.api.textures.FilterMode;
import com.mojang.renderpearl.api.textures.GpuTexture;
import com.mojang.renderpearl.api.textures.GpuTextureView;
import cursed.recode.render.GuiFrame;
import cursed.recode.render.Native;
import java.time.LocalTime;
import java.util.ArrayList;
import java.util.Iterator;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.regex.Matcher;
import java.util.regex.Pattern;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.Hud;
import net.minecraft.client.gui.components.LerpingBossEvent;
import net.minecraft.client.gui.components.PlayerFaceExtractor;
import net.minecraft.client.gui.screens.ChatScreen;
import net.minecraft.client.gui.screens.inventory.InventoryScreen;
import net.minecraft.client.player.AbstractClientPlayer;
import net.minecraft.client.renderer.RenderPipelines;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.util.ARGB;
import net.minecraft.world.effect.MobEffectInstance;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EquipmentSlot;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.item.ItemCooldowns;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.Level;
import org.jspecify.annotations.Nullable;

/**
 * The client's HUD (gui/hud.rs). Each frame, in the HUD's live stratum: what the game knows goes to the native side
 * (Native.hudFrame), its picture comes back in a texture of our own (drawn with the menu's, GuiFrame) and is put over
 * the game; then the icons it leaves room for (effects' sprites, items, the target's portrait) are drawn here.
 */
public final class HudLayer {
    private static @Nullable GpuTexture texture;
    private static @Nullable GpuTextureView view;
    /** What is open by an element (its settings, a list, the colour wheel): over everything, by the chat or editor. */
    private static @Nullable GpuTexture overTexture;
    private static @Nullable GpuTextureView overView;
    private static int overId;
    /** This frame drew the HUD again (not: the last drawing is shown), and the last drawing has something over it. */
    private static boolean redrew = true;
    private static boolean overShown;
    /** The HUD's texture holds a drawing (the last frame that drew had something to draw). */
    private static boolean shown;
    private static boolean init;
    private static final float[] STATE = new float[96];
    private static final float[] ICONS = new float[7 * 512];
    private static boolean hideEffects, wantEditor;
    /** The last one hit and when (MultiPlayerGameModeMixin). */
    private static @Nullable LivingEntity target;
    private static long targetAt;
    /** The cooldowns started (group, ticks), as ItemCooldowns says (ItemCooldownsMixin). */
    private static final Map<Identifier, Integer> COOLDOWNS = new LinkedHashMap<>();
    private static final List<MobEffectInstance> effects = new ArrayList<>();
    /** The effects shown last, and those that ended a moment ago (sent a little longer, marked gone, to fade out). */
    private static final Map<Object, MobEffectInstance> SHOWN = new LinkedHashMap<>();
    private static final Map<Object, Long> GONE = new LinkedHashMap<>();
    private static final List<ItemStack> cools = new ArrayList<>();
    private static final Pattern NUMBER = Pattern.compile("(\\d+)");

    private HudLayer() {
    }

    public static boolean hideEffects() {
        return hideEffects;
    }

    /** The menu's "Edit the layout" asked for the editor (once). */
    public static boolean wantsEditor() {
        Native.hudState(STATE);
        boolean w = wantEditor || STATE[1] > 0.5F;
        wantEditor = false;
        return w;
    }

    /** The player hit something (MultiPlayerGameModeMixin). */
    public static void attacked(Entity entity) {
        if (entity instanceof LivingEntity living) {
            target = living;
            targetAt = System.nanoTime();
            if (GuiFrame.active()) {
                Native.hudEvent(2, "");
            }
        }
    }

    /** A cooldown started for the player (ItemCooldownsMixin). */
    public static void cooldown(ItemCooldowns cooldowns, Identifier group, int ticks) {
        Minecraft mc = Minecraft.getInstance();
        if (mc.player != null && mc.player.getCooldowns() == cooldowns) {
            COOLDOWNS.remove(group);
            COOLDOWNS.put(group, Math.max(ticks, 1));
            cursed.recode.play.Adapt.started(group);
        }
    }

    /** A living thing's totem popped (LivingEntityHudMixin). */
    public static void totem(LivingEntity entity) {
        if (GuiFrame.active()) {
            Native.hudEvent(1, entity.getName().getString());
        }
    }

    /** Whether a boss bar is a server's PvP mode (by its name): its end makes a notification. */
    public static boolean combat(LerpingBossEvent event) {
        String n = event.getName().getString().toLowerCase(Locale.ROOT);
        return n.contains("pvp") || n.contains("пвп") || n.contains("combat") || n.contains("бой") || n.contains("боя") || n.contains("сраж");
    }

    public static int button(int sdl) {
        return switch (sdl) {
            case InputConstants.MOUSE_BUTTON_LEFT -> 0;
            case InputConstants.MOUSE_BUTTON_RIGHT -> 1;
            case InputConstants.MOUSE_BUTTON_MIDDLE -> 2;
            default -> sdl - 1;
        };
    }

    // ------------------------------------------------------------------------------------------ the frame
    /** The one fought, looked at each tick (Cosmetics.tick): its death after the hit a kill even when the HUD is not
     *  drawn again that frame (its picture kept), and gone after half a minute. */
    public static void watch(Minecraft mc) {
        LivingEntity t = target;
        if (t != null && (System.nanoTime() - targetAt > 30_000_000_000L || (t.isRemoved() && !t.isDeadOrDying()) || t.level() != mc.level)) {
            target = t = null;
        }
        HudVars.target(t, t == null ? Long.MAX_VALUE : System.nanoTime() - targetAt);
    }

    public static void extract(GuiGraphicsExtractor graphics, float partial) {
        Minecraft mc = Minecraft.getInstance();
        if (!GuiFrame.active() || mc.player == null || mc.level == null) {
            return;
        }
        if (!init) {
            init = true;
            Native.hudInit(mc.getUser().getName(), mc.gameDirectory.getAbsolutePath());
        }
        var win = mc.getWindow();
        int w = win.getWidth(), h = win.getHeight();
        if (w <= 0 || h <= 0) {
            return;
        }
        NChat.frame(mc);
        // drawn again at the screen's rate (HudCache.hudRedraw); between, the last drawing is shown with its icons. Not
        // under another screen: its panel (Custom Gui) is made with the HUD's frame
        var screen = mc.gui.screen();
        boolean sized = texture != null && texture.getWidth(0) == w && texture.getHeight(0) == h;
        redrew = !shown || !sized || (screen != null && !(screen instanceof ChatScreen)) || cursed.recode.scene.HudCache.hudRedraw(mc);
        if (!redrew) {
            graphics.blit(view, RenderSystem.getSamplerCache().getClampToEdge(FilterMode.NEAREST), 0, 0, graphics.guiWidth(), graphics.guiHeight(), 0.0F, 1.0F, 0.0F, 1.0F);
            icons(mc, graphics, (float) win.getGuiScale());
            return;
        }
        StringBuilder text = new StringBuilder();
        double[] nums = facts(mc, partial, text);
        // Shulker Preview's boxes over the world, where they are this frame (the HUD draws them)
        cursed.recode.fx.ShulkerPreview.frame(mc, partial, w, h);
        // the camera, for Server Adaptation's zones and timers (drawn on the HUD)
        cursed.recode.play.Adapt.view(mc, partial);
        // Name Tags' players this frame
        cursed.recode.fx.NameTags.frame(mc, partial);
        int id = Native.hudFrame(w, h, nums, text.toString());
        overId = Native.hudOverlay();
        Native.hudState(STATE);
        hideEffects = STATE[0] > 0.5F;
        wantEditor |= STATE[1] > 0.5F;
        HudBody.active = STATE[2] > 0.5F;
        HudBody.state(STATE);
        HudF3.active = STATE[81] > 0.5F;
        HudChat.state(STATE);
        HudBoard.state(STATE);
        HudOther.state(STATE);
        HudScreen.state(STATE);
        HudInv.state(STATE);
        HudPack.state(STATE);
        HudCreative.state(STATE);
        NChat.state(STATE);
        shown = id != 0;
        if (id == 0) {
            return;
        }
        if (texture == null || texture.getWidth(0) != w || texture.getHeight(0) != h) {
            release();
            texture = RenderSystem.getDevice().createTexture("Cursed HUD", GpuTexture.USAGE_COPY_DST | GpuTexture.USAGE_TEXTURE_BINDING, GpuFormat.RGBA8_UNORM, w, h, 1, 1);
            view = RenderSystem.getDevice().createTextureView(texture);
        }
        GuiFrame.sendHud(texture, id);
        graphics.blit(view, RenderSystem.getSamplerCache().getClampToEdge(FilterMode.NEAREST), 0, 0, graphics.guiWidth(), graphics.guiHeight(), 0.0F, 1.0F, 0.0F, 1.0F);
        icons(mc, graphics, (float) win.getGuiScale());
    }

    /** What is open by an element, over the whole GUI: at the end of the chat's or the editor's extraction. */
    public static void overlay(GuiGraphicsExtractor graphics) {
        // a frame that shows the last drawing shows what was over it too
        if (!redrew) {
            if (overShown && overView != null && GuiFrame.active()) {
                graphics.nextStratum();
                graphics.blit(overView, RenderSystem.getSamplerCache().getClampToEdge(FilterMode.NEAREST), 0, 0, graphics.guiWidth(), graphics.guiHeight(), 0.0F, 1.0F, 0.0F, 1.0F);
            }
            return;
        }
        int id = overId;
        overId = 0;
        overShown = id != 0 && GuiFrame.active();
        if (id == 0 || !GuiFrame.active()) {
            return;
        }
        var win = Minecraft.getInstance().getWindow();
        int w = win.getWidth(), h = win.getHeight();
        if (overTexture == null || overTexture.getWidth(0) != w || overTexture.getHeight(0) != h) {
            if (overView != null) {
                overView.close();
            }
            if (overTexture != null) {
                overTexture.close();
            }
            overTexture = RenderSystem.getDevice().createTexture("Cursed HUD settings", GpuTexture.USAGE_COPY_DST | GpuTexture.USAGE_TEXTURE_BINDING, GpuFormat.RGBA8_UNORM, w, h, 1, 1);
            overView = RenderSystem.getDevice().createTextureView(overTexture);
        }
        graphics.nextStratum();
        GuiFrame.send(overTexture, id);
        graphics.blit(overView, RenderSystem.getSamplerCache().getClampToEdge(FilterMode.NEAREST), 0, 0, graphics.guiWidth(), graphics.guiHeight(), 0.0F, 1.0F, 0.0F, 1.0F);
    }

    private static void release() {
        if (view != null) {
            view.close();
            view = null;
        }
        if (texture != null) {
            texture.close();
            texture = null;
        }
    }

    /** What the HUD shows, as hud.rs Facts::parse reads it. */
    private static double[] facts(Minecraft mc, float partial, StringBuilder text) {
        var p = mc.player;
        effects.clear();
        cools.clear();
        List<Double> n = new ArrayList<>(64);
        for (int i = 0; i < 24; i++) {
            n.add(0.0);
        }
        n.set(0, (double) mc.getFps());
        var info = mc.getConnection() != null ? mc.getConnection().getPlayerInfo(p.getUUID()) : null;
        n.set(1, info != null ? (double) info.getLatency() : 0.0);
        n.set(2, p.getX());
        n.set(3, p.getY());
        n.set(4, p.getZ());
        var dim = mc.level.dimension();
        n.set(5, dim == Level.NETHER ? 1.0 : dim == Level.END ? 2.0 : 0.0);
        n.set(6, (double) p.getYRot());
        var screen = mc.gui.screen();
        int flags = 0;
        if (screen instanceof ChatScreen) {
            flags |= 1;
        }
        // F3 itself open (the game's showDebugScreen is true with F3+G or F3+B on too)
        if (mc.debugEntries.isOverlayVisible() && (!mc.gui.hud.isHidden() || screen != null)) {
            flags |= 2;
        }
        if (screen != null && !(screen instanceof ChatScreen) && !(screen instanceof HudEditor)) {
            flags |= 4;
        }
        if (screen instanceof HudEditor) {
            flags |= 8;
        }
        // the way into the world or out of it plays over the game: the HUD away, back in after it
        if (cursed.recode.launch.Transition.entering() || cursed.recode.launch.Transition.exiting()) {
            flags |= 16;
        }
        n.set(7, (double) flags);
        n.set(8, mc.mouseHandler.xpos());
        n.set(9, mc.mouseHandler.ypos());
        n.set(16, (double) LocalTime.now().toSecondOfDay());
        var server = mc.getCurrentServer();
        text.append(server != null ? server.ip : mc.isLocalServer() ? "Singleplayer" : "").append('\n');
        // effects, as the game sorts them; then those that ended in the last 0.6 s (the last slot 2: gone)
        long now = System.nanoTime();
        List<MobEffectInstance> active = p.getActiveEffects().stream().filter(MobEffectInstance::showIcon).sorted(java.util.Comparator.reverseOrder()).toList();
        java.util.Set<Object> on = new java.util.HashSet<>();
        for (MobEffectInstance e : active) {
            on.add(e.getEffect());
        }
        for (Object k : SHOWN.keySet()) {
            if (!on.contains(k) && !GONE.containsKey(k)) {
                GONE.put(k, now);
            }
        }
        GONE.keySet().removeIf(on::contains);
        GONE.entrySet().removeIf(en -> {
            boolean old = now - en.getValue() > 600_000_000L;
            if (old) {
                SHOWN.remove(en.getKey());
            }
            return old;
        });
        for (MobEffectInstance e : active) {
            SHOWN.put(e.getEffect(), e);
            effect(e, false, n, text);
        }
        for (Object k : GONE.keySet()) {
            MobEffectInstance e = SHOWN.get(k);
            if (e != null) {
                effect(e, true, n, text);
            }
        }
        n.set(10, (double) effects.size());
        // cooldowns: the groups started, each by an item of the player's of that group (or the group's own item)
        var cd = p.getCooldowns();
        Iterator<Map.Entry<Identifier, Integer>> it = COOLDOWNS.entrySet().iterator();
        while (it.hasNext()) {
            var en = it.next();
            ItemStack stack = ItemStack.EMPTY;
            var inv = p.getInventory();
            for (int i = 0; i < inv.getContainerSize() && stack.isEmpty(); i++) {
                ItemStack s = inv.getItem(i);
                if (!s.isEmpty() && cd.getCooldownGroup(s).equals(en.getKey())) {
                    stack = s;
                }
            }
            if (stack.isEmpty()) {
                var item = BuiltInRegistries.ITEM.getValue(en.getKey());
                stack = item == null ? ItemStack.EMPTY : new ItemStack(item);
            }
            float frac = stack.isEmpty() ? 0.0F : cd.getCooldownPercent(stack, partial);
            if (frac <= 0.0F) {
                it.remove();
                continue;
            }
            cools.add(stack);
            text.append(stack.getHoverName().getString()).append('\n');
            n.add((double) frac);
            n.add(frac * en.getValue() / 20.0);
        }
        n.set(11, (double) cools.size());
        // the one fought: hit in the last half minute, still there
        LivingEntity t = target;
        if (t != null && (System.nanoTime() - targetAt > 30_000_000_000L || (t.isRemoved() && !t.isDeadOrDying()) || t.level() != mc.level)) {
            target = t = null;
        }
        HudVars.target(t, t == null ? Long.MAX_VALUE : System.nanoTime() - targetAt);
        if (t != null) {
            n.set(12, 1.0);
            text.append(net.minecraft.ChatFormatting.stripFormatting(t.getName().getString())).append('\n');
            int bits = 0;
            EquipmentSlot[] slots = {EquipmentSlot.HEAD, EquipmentSlot.CHEST, EquipmentSlot.LEGS, EquipmentSlot.FEET};
            for (int k = 0; k < 4; k++) {
                if (!t.getItemBySlot(slots[k]).isEmpty()) {
                    bits |= 1 << k;
                }
            }
            n.add((double) t.getHealth());
            n.add((double) t.getMaxHealth());
            n.add((double) t.getAbsorptionAmount());
            n.add((double) bits);
            n.add((double) p.distanceTo(t));
            n.add(t.isDeadOrDying() ? 1.0 : 0.0);
            n.add((double) t.getId());
            n.add((System.nanoTime() - targetAt) / 1e9);
            n.add(t instanceof AbstractClientPlayer ? 1.0 : 0.0);
        }
        // the player's body for the Custom Hotbar (gui/hudbar.rs): its numbers, the held item's name
        n.set(17, 1.0);
        HudBody.append(mc, p, partial, n, text);
        // the Custom Chat's lines (gui/hudchat.rs): where each is, how seen, its time
        if (HudChat.active()) {
            n.set(18, 1.0);
            HudChat.append(mc, n);
        }
        // the Custom Scoreboard (gui/hudboard.rs): its title and lines as the server sent them, what changed
        if (HudBoard.active()) {
            n.set(19, 1.0);
            HudBoard.append(mc, n, text);
        }
        // Other Custom Things (gui/hudother.rs): the boss bars, the title, the action bar
        if (HudOther.bars() || HudOther.titles() || HudOther.action()) {
            n.set(20, 1.0);
            HudOther.append(mc, partial, n, text);
        }
        // Custom Gui (gui/hudscreen.rs): the open container's picture, its slots, the hovered one
        n.set(21, 1.0);
        HudScreen.append(mc, n);
        // a server's PvP mode, from its boss bar (the notification of its end)
        n.set(13, -1.0);
        var bars = ((cursed.recode.mixin.client.BossHealthOverlayAccess) mc.gui.hud.getBossOverlay()).cursed$events();
        for (LerpingBossEvent ev : bars.values()) {
            if (combat(ev)) {
                Matcher m = NUMBER.matcher(ev.getName().getString());
                float frac = ev.getProgress();
                n.set(13, m.find() ? Double.parseDouble(m.group(1)) : frac * 30.0);
                n.set(14, (double) frac);
                break;
            }
        }
        // the placeholders' values, after a line of U+0001 (gui/hud.rs: {name} in any text of the HUD)
        HudVars.append(mc, t, text);
        double[] out = new double[n.size()];
        for (int i = 0; i < out.length; i++) {
            out[i] = n.get(i);
        }
        return out;
    }

    private static void effect(MobEffectInstance e, boolean gone, List<Double> n, StringBuilder text) {
        effects.add(e);
        text.append(e.getEffect().value().getDisplayName().getString()).append('\n');
        n.add((double) e.getAmplifier());
        n.add(gone ? 0.0 : e.isInfiniteDuration() ? -1.0 : e.getDuration() / 20.0);
        n.add(e.getEffect().value().isBeneficial() ? 0.0 : 1.0);
        n.add(gone ? 2.0 : e.isAmbient() ? 1.0 : 0.0);
    }

    private static boolean tinySaid;

    /** The icons over the HUD's picture, where it left room for them. */
    /** The icons over the open container's panel (Custom Gui: the player's pictures there). */
    static void screenIcons(Minecraft mc, GuiGraphicsExtractor graphics) {
        icons(mc, graphics, (float) mc.getWindow().getGuiScale(), true);
    }

    private static void icons(Minecraft mc, GuiGraphicsExtractor graphics, float scale) {
        icons(mc, graphics, scale, false);
    }

    private static void icons(Minecraft mc, GuiGraphicsExtractor graphics, float scale, boolean screen) {
        int count = screen ? Native.hudScreenIcons(ICONS) : Native.hudIcons(ICONS);
        // a part of the next icon only (99: the HUD cut it around what covers it, as the open chat's window)
        float[] clip = null;
        for (int i = 0; i < count; i++) {
            int o = i * 7;
            int kind = (int) ICONS[o];
            int index = (int) ICONS[o + 1];
            float x = ICONS[o + 2] / scale, y = ICONS[o + 3] / scale, w = ICONS[o + 4] / scale, h = ICONS[o + 5] / scale;
            float alpha = ICONS[o + 6];
            if (kind == 99) {
                clip = new float[] {x, y, x + w, y + h};
                continue;
            }
            // the part in whole GUI px: its edges that are the icon's go out (all of it shows), those a cover made go
            // in (nothing of it over the cover)
            int[] part = null;
            if (clip != null) {
                float e = 0.01F;
                part = new int[] {
                        (int) (Math.abs(clip[0] - x) < e ? Math.floor(clip[0]) : Math.ceil(clip[0])),
                        (int) (Math.abs(clip[1] - y) < e ? Math.floor(clip[1]) : Math.ceil(clip[1])),
                        (int) (Math.abs(clip[2] - (x + w)) < e ? Math.ceil(clip[2]) : Math.floor(clip[2])),
                        (int) (Math.abs(clip[3] - (y + h)) < e ? Math.ceil(clip[3]) : Math.floor(clip[3]))};
                clip = null;
            }
            if (alpha < 0.02F) {
                continue;
            }
            // items and models are drawn opaque: as their element fades they shrink into their middle instead
            float k = alpha >= 0.98F ? 1.0F : Math.max(0.0F, (alpha - 0.2F) / 0.78F);
            k = k * k * (3.0F - 2.0F * k);
            if (kind != 1 && kind != 7 && kind != 8) {
                if (k < 0.05F) {
                    continue;
                }
                x += w * (1.0F - k) / 2.0F;
                y += h * (1.0F - k) / 2.0F;
                w *= k;
                h *= k;
            }
            if (part != null) {
                if (part[2] > part[0] && part[3] > part[1]) {
                    graphics.enableScissor(part[0], part[1], part[2], part[3]);
                    one(mc, graphics, kind, index, x, y, w, h, alpha);
                    graphics.disableScissor();
                }
                continue;
            }
            one(mc, graphics, kind, index, x, y, w, h, alpha);
        }
    }

    /** One of the icons Java draws (hud.rs says which kind is what), at its place in the GUI's px. */
    private static void one(Minecraft mc, GuiGraphicsExtractor graphics, int kind, int index, float x, float y, float w, float h, float alpha) {
        {
            switch (kind) {
                case 1 -> {
                    if (index < effects.size()) {
                        var pose = graphics.pose();
                        pose.pushMatrix();
                        pose.translate(x, y);
                        pose.scale(w / 18.0F, h / 18.0F);
                        graphics.blitSprite(RenderPipelines.GUI_TEXTURED, Hud.getMobEffectSprite(effects.get(index).getEffect()), 0, 0, 18, 18, ARGB.white(alpha));
                        pose.popMatrix();
                    }
                }
                case 2 -> {
                    if (index < cools.size()) {
                        item(graphics, cools.get(index), x, y, w);
                    }
                }
                case 3, 4 -> {
                    // index 1: the player themself (the element's example in the chat and the editor)
                    LivingEntity t = index == 1 ? mc.player : target;
                    if (t == null) {
                        return;
                    }
                    if (kind == 4 && t instanceof AbstractClientPlayer pl) {
                        int s = Math.max(1, Math.round(w * 0.8F));
                        PlayerFaceExtractor.extractRenderState(graphics, pl.getSkin(), Math.round(x + (w - s) / 2.0F), Math.round(y + (h - s) / 2.0F), s);
                    } else {
                        int x0 = Math.round(x), y0 = Math.round(y), x1 = Math.round(x + w), y1 = Math.round(y + h);
                        // not while the box is under a pixel (it grows in: the game draws the model into a texture of
                        // its size, and a texture of none is a crash — 12.10, hitting a mob at thousands of FPS)
                        if (x1 <= x0 || y1 <= y0) {
                            if (cursed.recode.dev.Dev.on("CURSED_DEV_HUDHITS") && !tinySaid) {
                                tinySaid = true;
                                org.slf4j.LoggerFactory.getLogger("HudLayer").info("[dev] hud: the target's model under a pixel, left out");
                            }
                            return;
                        }
                        // head and shoulders: the box shows the top ~60% of it, centred a little below the eyes
                        float bb = Math.max(0.5F, t.getBbHeight());
                        float size = h / (0.62F * bb);
                        InventoryScreen.extractEntityInInventoryFollowsMouse(graphics, x0, y0, x1, y1, Math.max(1, Math.round(size)), 0.3F * bb,
                                x1 + 40.0F, y0 + h * 0.3F, t);
                    }
                }
                case 5 -> {
                    // 0..3 the target's armour, 4..7 the player's own (the example)
                    LivingEntity t = index >= 4 ? mc.player : target;
                    EquipmentSlot[] slots = {EquipmentSlot.HEAD, EquipmentSlot.CHEST, EquipmentSlot.LEGS, EquipmentSlot.FEET};
                    if (t != null && index >= 0 && index < 8) {
                        item(graphics, t.getItemBySlot(slots[index % 4]), x, y, w);
                    }
                }
                case 6 -> item(graphics, new ItemStack(Items.TOTEM_OF_UNDYING), x, y, w);
                // the Custom Hotbar's slots (their counts, wear and cooldowns are drawn by the HUD itself)
                case 9 -> item(graphics, HudBody.slot(index), x, y, w);
                // Auto Swap's thing (its note, the Swap Ring)
                case 10 -> item(graphics, cursed.recode.swap.AutoSwap.shown(index), x, y, w);
                // Shulker Preview's boxes over the world (gui/hudshulker.rs)
                case 11 -> item(graphics, cursed.recode.fx.ShulkerPreview.icon(index), x, y, w);
                case 7 -> HudPictures.draw(graphics, index, x, y, w, h, alpha);
                // the client's own chat: faces, a server's pack glyphs, the items it shows, its cards' items
                case 20, 21, 22, 23, 24, 26, 27 -> NChat.icon(graphics, kind, index, x, y, w, h, alpha);
                // a server's glyphs in the HUD's words (the sidebar, the boss bars, the title, the action bar)
                case 25 -> HudText.icon(graphics, index, x, y, w, h, alpha);
                case 8 -> HudPictures.draw(graphics, -1, x, y, w, h, alpha);
                default -> {
                }
            }
        }
    }

    private static void item(GuiGraphicsExtractor graphics, ItemStack stack, float x, float y, float size) {
        if (stack.isEmpty()) {
            return;
        }
        var pose = graphics.pose();
        pose.pushMatrix();
        pose.translate(x, y);
        pose.scale(size / 16.0F, size / 16.0F);
        graphics.item(stack, 0, 0);
        pose.popMatrix();
    }
}
