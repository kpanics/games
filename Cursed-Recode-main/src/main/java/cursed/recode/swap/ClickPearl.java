package cursed.recode.swap;

import cursed.recode.render.Native;
import java.util.HashMap;
import java.util.Map;
import net.minecraft.client.Minecraft;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.client.resources.sounds.SimpleSoundInstance;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.network.protocol.game.ServerboundContainerClosePacket;
import net.minecraft.sounds.SoundEvents;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.inventory.ContainerInput;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.ClipContext;
import net.minecraft.world.phys.HitResult;
import net.minecraft.world.phys.Vec3;

/**
 * Click Pearl (the menu's "click-pearl"; its keys are the menu's, gui/swap.rs PEARL, and come as "pearlev"): each
 * press throws an ender pearl — from the off hand ("Off hand first", else when there is no other), the hand, the
 * hotbar or, but on ReallyWorld or with "Only from the hotbar", the whole inventory; of several stacks the nearest (the
 * hotbar's first), the smallest or the largest — and what was held comes back. Only on a press: none is kept for later
 * (a pearl cooling down, none there: a quiet click, if asked). "One tick": the slot taken, the throw and the slot given
 * back in one tick; "Like by hand": the slot taken, the throw the next tick, given back two ticks after (the pearl seen
 * in the hand, as a player's). Never in a menu, never while the hands are busy (eating, a shield up: the game would not
 * either). "Not point-blank": a block nearer than its distance in the way, it does not go — but in a trap (a block over
 * the head, two sides closed), with another player near, or pressed again within a second it always does. The menu
 * hides it where the server bans it (FunTime, AresMine, HolyWorld).
 */
public final class ClickPearl {
    private static final String MODULE = "click-pearl";
    /** CURSED_DEV_PEARL: each throw, in the log. */
    private static final boolean LOG = cursed.recode.dev.Dev.on("CURSED_DEV_PEARL");
    private static final int OFF_HAND = -2;
    private static final int MAIN_HAND = -3;
    /** How near another player lets a point-blank throw go; how long a second press overrules it (ticks). */
    private static final double NEAR = 6.0;
    private static final int AGAIN = 20;

    private static Map<String, String> set = Map.of();
    private static long setAt;
    private static long tick;
    private static long refusedAt = -1000;
    /** A throw "like by hand" under way: 1 the throw waits, 2 the giving back waits; the slot it is from (0..35), the
     *  slot held before, the ticks left, the ticks it may wait for a menu to close. */
    private static int step;
    private static int from = -1;
    private static int held = -1;
    private static int wait;
    private static int patience;

    private ClickPearl() {
    }

    /** Each game tick (MinecraftMixin). */
    public static void tick(Minecraft mc) {
        tick++;
        // (the presses always taken: one pressed while it could not go is not done later)
        String ev = Native.guiCall("pearlev");
        int presses = 0;
        try {
            presses = ev == null || ev.isEmpty() ? 0 : Integer.parseInt(ev.trim());
        } catch (NumberFormatException ignored) {
            // none
        }
        LocalPlayer p = mc.player;
        if (p == null || mc.gameMode == null || mc.level == null) {
            step = 0;
            return;
        }
        poll();
        if (step != 0) {
            advance(mc, p);
            return;
        }
        if (presses > 0 && "1".equals(set.get("on"))) {
            press(mc, p);
        }
    }

    private static void press(Minecraft mc, LocalPlayer p) {
        if (mc.gui.screen() != null || p.isSpectator() || p.isHandsBusy() || p.isUsingItem() || p.containerMenu != p.inventoryMenu) {
            return;
        }
        boolean hotbarOnly = Servers.REALLYWORLD.equals(Servers.now(mc)) || "1".equals(set.get("Only from the hotbar"));
        if (p.getCooldowns().isOnCooldown(new ItemStack(Items.ENDER_PEARL))) {
            refused("cooling down");
            none(mc);
            return;
        }
        int src = source(p, hotbarOnly);
        if (src == -1) {
            refused("no pearl");
            none(mc);
            return;
        }
        if ("1".equals(set.get("Not point-blank")) && tick - refusedAt > AGAIN && pointBlank(mc, p)) {
            refusedAt = tick;
            refused("point-blank");
            none(mc);
            return;
        }
        refusedAt = -1000;
        boolean byHand = "Like by hand".equals(set.get("Speed"));
        boolean back = !"0".equals(set.get("Back to the held slot"));
        int sel = p.getInventory().getSelectedSlot();
        if (LOG) {
            cursed.recode.CursedRecode.LOGGER.info("[click pearl] tick {}: from {}, {}, hotbar only {}", tick,
                    src == OFF_HAND ? "the off hand" : src == MAIN_HAND ? "the hand" : "slot " + src, byHand ? "like by hand" : "one tick", hotbarOnly);
        }
        if (src == OFF_HAND || src == MAIN_HAND) {
            use(mc, p, src == OFF_HAND ? InteractionHand.OFF_HAND : InteractionHand.MAIN_HAND);
            after(p);
            return;
        }
        // the slot taken: a hotbar's picked, one above it swapped into the hand's
        from = src;
        held = sel;
        if (src < 9) {
            select(mc, p, src);
        } else {
            swap(mc, p, src, sel);
        }
        if (!byHand) {
            use(mc, p, InteractionHand.MAIN_HAND);
            giveBack(mc, p, back);
            after(p);
            return;
        }
        step = 1;
        wait = 1;
        patience = 40;
    }

    /** A throw "like by hand": the throw the tick after the slot was taken, the giving back two ticks after it. */
    private static void advance(Minecraft mc, LocalPlayer p) {
        if (--wait > 0) {
            return;
        }
        if (p.containerMenu != p.inventoryMenu && --patience > 0) {
            // a container opened meanwhile: its slots are not the inventory's, it waits for it to close
            wait = 1;
            return;
        }
        if (step == 1) {
            if (p.getMainHandItem().is(Items.ENDER_PEARL) && !p.isUsingItem()) {
                use(mc, p, InteractionHand.MAIN_HAND);
            }
            step = 2;
            wait = 2;
            return;
        }
        step = 0;
        giveBack(mc, p, !"0".equals(set.get("Back to the held slot")));
        after(p);
    }

    private static void refused(String why) {
        note("not thrown, " + why);
    }

    private static void note(String what) {
        if (LOG) {
            cursed.recode.CursedRecode.LOGGER.info("[click pearl] tick {}: {}", tick, what);
        }
    }

    /** The log: what is held after a throw, how many pearls are left, where. */
    private static void after(LocalPlayer p) {
        if (!LOG) {
            return;
        }
        int left = p.getOffhandItem().is(Items.ENDER_PEARL) ? p.getOffhandItem().getCount() : 0;
        StringBuilder where = new StringBuilder();
        for (int i = 0; i < 36; i++) {
            ItemStack st = p.getInventory().getItem(i);
            if (st.is(Items.ENDER_PEARL)) {
                left += st.getCount();
                where.append(' ').append(i).append(':').append(st.getCount());
            }
        }
        cursed.recode.CursedRecode.LOGGER.info("[click pearl] tick {}: thrown; held slot {} ({}), pearls left {} (off hand {};{})", tick,
                p.getInventory().getSelectedSlot(), p.getMainHandItem().getHoverName().getString(), left, p.getOffhandItem().getCount(), where);
    }

    /** What was held comes back: the stack above the hotbar to its slot (always), the hotbar's slot held before (when
     *  asked, and when the slot thrown from is still the one held). */
    private static void giveBack(Minecraft mc, LocalPlayer p, boolean back) {
        if (from >= 9) {
            swap(mc, p, from, held);
        } else if (back && held >= 0 && held != from && p.getInventory().getSelectedSlot() == from) {
            select(mc, p, held);
        }
        from = -1;
        held = -1;
    }

    /** Where the pearl comes from: the off hand (first, if asked), the hand, the stack the setting picks, the off hand
     *  last; -1 none. */
    private static int source(LocalPlayer p, boolean hotbarOnly) {
        boolean offFirst = !"0".equals(set.get("Off hand first"));
        if (offFirst && p.getOffhandItem().is(Items.ENDER_PEARL)) {
            return OFF_HAND;
        }
        if (p.getMainHandItem().is(Items.ENDER_PEARL)) {
            return MAIN_HAND;
        }
        String pick = set.getOrDefault("Stack", "Nearest");
        int sel = p.getInventory().getSelectedSlot();
        int found = -1;
        double best = Double.MAX_VALUE;
        for (int i = 0; i < (hotbarOnly ? 9 : 36); i++) {
            ItemStack st = p.getInventory().getItem(i);
            if (!st.is(Items.ENDER_PEARL)) {
                continue;
            }
            // a lower score is taken; the hotbar's before the rest, then the nearer to the slot held
            double near = i < 9 ? Math.abs(i - sel) : 9 + i;
            double score = switch (pick) {
                case "Smallest" -> st.getCount() * 100.0 + near;
                case "Largest" -> -st.getCount() * 100.0 + near;
                default -> near;
            };
            if (score < best) {
                best = score;
                found = i;
            }
        }
        if (found < 0 && !offFirst && p.getOffhandItem().is(Items.ENDER_PEARL)) {
            return OFF_HAND;
        }
        return found;
    }

    /**
     * Whether a block is in the way nearer than "Point-blank distance" — and it is not a trap (a block over the head,
     * two sides closed at the feet or the head: a pearl into the wall or the floor is how one gets out) and no other
     * player is near (a fight: it may be meant).
     */
    private static boolean pointBlank(Minecraft mc, LocalPlayer p) {
        double dist = 2.0;
        try {
            dist = Double.parseDouble(set.getOrDefault("Point-blank distance", "2"));
        } catch (NumberFormatException ignored) {
            // as made
        }
        Vec3 eye = p.getEyePosition();
        Vec3 to = eye.add(p.getViewVector(1.0F).scale(dist));
        if (mc.level.clip(new ClipContext(eye, to, ClipContext.Block.COLLIDER, ClipContext.Fluid.NONE, p)).getType() == HitResult.Type.MISS) {
            return false;
        }
        if (trapped(mc, p)) {
            note("point-blank, but a trap: it goes");
            return false;
        }
        for (var other : mc.level.players()) {
            if (other != p && !other.isSpectator() && other.distanceTo(p) <= NEAR) {
                note("point-blank, but " + other.getName().getString() + " is near: it goes");
                return false;
            }
        }
        return true;
    }

    private static boolean trapped(Minecraft mc, LocalPlayer p) {
        BlockPos feet = p.blockPosition();
        BlockPos head = feet.above();
        if (solid(mc, head.above())) {
            return true;
        }
        int closed = 0;
        for (Direction d : Direction.Plane.HORIZONTAL) {
            if (solid(mc, feet.relative(d)) || solid(mc, head.relative(d))) {
                closed++;
            }
        }
        return closed >= 2;
    }

    private static boolean solid(Minecraft mc, BlockPos at) {
        return !mc.level.getBlockState(at).getCollisionShape(mc.level, at).isEmpty();
    }

    /** The pearl in `hand` thrown as the game does it on the right button: used, the arm swung, the use counted. */
    private static void use(Minecraft mc, LocalPlayer p, InteractionHand hand) {
        ItemStack st = p.getItemInHand(hand);
        if (!st.is(Items.ENDER_PEARL)) {
            return;
        }
        var swing = st.getInteractAnimation();
        if (mc.gameMode.useItem(p, hand) instanceof InteractionResult.Success s) {
            if (s.swingSource() == InteractionResult.SwingSource.PREDICTED) {
                p.swing(hand, swing, false);
            }
            p.itemUsed(hand);
        }
    }

    /** The hotbar's slot `slot` held, the server told. */
    private static void select(Minecraft mc, LocalPlayer p, int slot) {
        p.getInventory().setSelectedSlot(slot);
        ((cursed.recode.mixin.client.MultiPlayerGameModeAccess) mc.gameMode).cursed$sendCarried();
    }

    /** The stack in slot `slot` (above the hotbar) and the hotbar's `via` change places, the inventory told closed. */
    private static void swap(Minecraft mc, LocalPlayer p, int slot, int via) {
        int id = p.inventoryMenu.containerId;
        mc.gameMode.handleContainerInput(id, Moves.menuSlot(slot), via, ContainerInput.SWAP, p);
        if (mc.gui.screen() == null) {
            p.connection.send(new ServerboundContainerClosePacket(id));
        }
    }

    /** A press that could not throw: a quiet click, if asked. */
    private static void none(Minecraft mc) {
        if (!"0".equals(set.get("Sound when there is none"))) {
            mc.getSoundManager().play(SimpleSoundInstance.forUI(SoundEvents.UI_BUTTON_CLICK.value(), 1.8F, 0.25F));
        }
    }

    private static void poll() {
        long now = System.nanoTime();
        if (now - setAt < 250_000_000L) {
            return;
        }
        setAt = now;
        Map<String, String> v = new HashMap<>();
        String s = Native.guiCall("set\t" + MODULE);
        if (s != null && !s.isEmpty()) {
            String[] f = s.split("\t");
            v.put("on", f[0]);
            for (int i = 1; i < f.length; i++) {
                int eq = f[i].indexOf('=');
                if (eq > 0) {
                    v.put(f[i].substring(0, eq), f[i].substring(eq + 1));
                }
            }
        }
        set = v;
    }
}
