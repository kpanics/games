package cursed.recode.play;

import cursed.recode.render.Native;
import java.lang.ref.WeakReference;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import net.minecraft.client.Minecraft;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.protocol.game.ServerboundContainerClosePacket;
import net.minecraft.resources.Identifier;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.inventory.ContainerInput;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.Level;

/**
 * Server Adaptation (native gui/adapt.rs decides): its facts told with the acting modules' each tick (what the hands
 * hold, the other players near, the cooldowns just started), the camera each frame (its zones and timers are drawn on
 * the HUD), a line of the chat asked about before it goes, and a thing of a server's used when its key asks ("use":
 * from a hand, the hotbar or — but on ReallyWorld itself — the inventory, sneaking when it is used so, what was held
 * given back).
 */
public final class Adapt {
    /** The cooldowns started since the last tick (their groups). */
    private static final List<String> STARTED = new ArrayList<>();
    private static WeakReference<Level> observedLevel = new WeakReference<>(null);
    /** A use waiting for the sneak to reach the server first: the item, from the hotbar only, the ticks left. */
    private static String waiting;
    private static String waitingName = "";
    private static LocalPlayer waitingPlayer;
    private static Level waitingLevel;
    private static boolean waitingHotbar;
    private static int waitTicks;
    private static boolean sneakPut;

    private Adapt() {
    }

    /** A cooldown started for the player (HudLayer.cooldown). */
    public static void started(Identifier group) {
        Level level = Minecraft.getInstance().level;
        trackLevel(level);
        if (level != null && STARTED.size() < 16) {
            STARTED.add(group.toString());
        }
    }

    private static void trackLevel(Level level) {
        if (observedLevel.get() != level) {
            STARTED.clear();
            observedLevel = new WeakReference<>(level);
        }
    }

    private static String id(ItemStack s) {
        return s.isEmpty() ? "" : BuiltInRegistries.ITEM.getKey(s.getItem()).toString();
    }

    /** The tick's facts (Play.tick puts them with its own). */
    static void facts(Minecraft mc, LocalPlayer p, java.util.function.BiConsumer<String, Object> put) {
        trackLevel(mc.level);
        put.accept("held", id(p.getMainHandItem()));
        put.accept("off", id(p.getOffhandItem()));
        put.accept("started", String.join(",", STARTED));
        STARTED.clear();
        StringBuilder others = new StringBuilder();
        for (Player o : mc.level.players()) {
            if (o == p || o.isSpectator() || o.distanceToSqr(p) > 48 * 48) {
                continue;
            }
            if (!others.isEmpty()) {
                others.append(';');
            }
            others.append(o.getScoreboardName()).append(' ')
                    .append(String.format(Locale.ROOT, "%.2f %.2f %.2f", o.getX(), o.getY(), o.getZ()));
        }
        put.accept("others", others);
    }

    /** Each frame, before the HUD is drawn: the camera (where, its turn, the projection) and the player's feet and look. */
    public static void view(Minecraft mc, float partial) {
        LocalPlayer p = mc.player;
        if (p == null) {
            return;
        }
        var cam = mc.gameRenderer.mainCamera();
        var c = cam.position();
        var q = cam.rotation();
        float[] m = new float[16];
        cursed.recode.render.NativeTerrain.PROJECTION.get(m);
        StringBuilder b = new StringBuilder(512);
        b.append(c.x).append(' ').append(c.y).append(' ').append(c.z).append(' ')
                .append(q.x()).append(' ').append(q.y()).append(' ').append(q.z()).append(' ').append(q.w());
        for (float v : m) {
            b.append(' ').append(v);
        }
        var feet = p.getPosition(partial);
        b.append(' ').append(feet.x).append(' ').append(feet.y).append(' ').append(feet.z)
                .append(' ').append(p.getViewYRot(partial)).append(' ').append(p.getViewXRot(partial));
        Native.guiCall("play\tview\t" + b);
    }

    /** A line about to be sent from the chat: held back (true) when the filter asks. */
    public static boolean heldBack(String line) {
        return line != null && !line.isBlank() && "1".equals(Native.guiCall("play\tsay?\t" + line.replace('\t', ' ')));
    }

    /** Each tick: a use waiting for its sneak, done. */
    static void tick(Minecraft mc) {
        LocalPlayer p = mc.player;
        trackLevel(mc.level);
        if (p == null || mc.level == null) {
            STARTED.clear();
            cancelWaiting(mc);
            return;
        }
        if (waiting == null) {
            return;
        }
        if (p == null || p != waitingPlayer || mc.level != waitingLevel || mc.gameMode == null || mc.gui.screen() != null || p.isUsingItem() || p.isDeadOrDying()) {
            cancelWaiting(mc);
            return;
        }
        if (--waitTicks > 0) {
            return;
        }
        if (!"1".equals(Native.guiCall("play\tadapt-ready\t" + waiting))) {
            cancelWaiting(mc);
            return;
        }
        String item = waiting;
        try {
            use(mc, p, item, waitingHotbar, waitingName);
        } finally {
            cancelWaiting(mc);
        }
    }

    private static void cancelWaiting(Minecraft mc) {
        waiting = null;
        waitingName = "";
        waitingPlayer = null;
        waitingLevel = null;
        if (sneakPut) {
            sneakPut = false;
            mc.options.keyShift.setDown(false);
        }
    }

    /** "use <item> <hotbar only> <sneak>[US exact name]": use a matching server item. */
    static void ask(Minecraft mc, String rest) {
        LocalPlayer p = mc.player;
        String[] request = rest.split("\u001f", 2);
        String[] f = request[0].split(" ");
        String name = request.length > 1 ? request[1] : "";
        if (p == null || mc.level == null || f.length < 3 || mc.gameMode == null || mc.gui.screen() != null || p.isUsingItem() || p.isDeadOrDying()) {
            return;
        }
        boolean hotbar = "1".equals(f[1]);
        if ("1".equals(f[2]) && !p.isShiftKeyDown()) {
            // the sneak goes to the server with the next tick's input; the use the tick after
            sneakPut |= !mc.options.keyShift.isDown();
            if (sneakPut) {
                mc.options.keyShift.setDown(true);
            }
            waiting = f[0];
            waitingName = name;
            waitingPlayer = p;
            waitingLevel = mc.level;
            waitingHotbar = hotbar;
            waitTicks = 2;
            return;
        }
        use(mc, p, f[0], hotbar, name);
    }

    private static boolean matches(ItemStack stack, String item, String name) {
        if (!id(stack).equals(item)) {
            return false;
        }
        return name.isEmpty() || "1".equals(Native.guiCall("play\titem-name\t" + name + "\u001f" + stack.getHoverName().getString().replace('\u001f', ' ')));
    }

    private static void use(Minecraft mc, LocalPlayer p, String item, boolean hotbarOnly, String name) {
        if (matches(p.getOffhandItem(), item, name)) {
            useHand(mc, p, InteractionHand.OFF_HAND);
            return;
        }
        if (matches(p.getMainHandItem(), item, name)) {
            useHand(mc, p, InteractionHand.MAIN_HAND);
            return;
        }
        var inv = p.getInventory();
        int sel = inv.getSelectedSlot();
        int from = -1;
        for (int i = 0; i < (hotbarOnly ? 9 : 36) && from < 0; i++) {
            if (matches(inv.getItem(i), item, name)) {
                from = i;
            }
        }
        if (from < 0) {
            return;
        }
        if (from < 9) {
            select(mc, p, from);
            useHand(mc, p, InteractionHand.MAIN_HAND);
            select(mc, p, sel);
        } else {
            swap(mc, p, from, sel);
            useHand(mc, p, InteractionHand.MAIN_HAND);
            swap(mc, p, from, sel);
        }
    }

    private static void useHand(Minecraft mc, LocalPlayer p, InteractionHand hand) {
        ItemStack st = p.getItemInHand(hand);
        var swing = st.getInteractAnimation();
        if (mc.gameMode.useItem(p, hand) instanceof InteractionResult.Success s) {
            if (s.swingSource() == InteractionResult.SwingSource.PREDICTED) {
                p.swing(hand, swing, false);
            }
            p.itemUsed(hand);
        }
    }

    private static void select(Minecraft mc, LocalPlayer p, int slot) {
        p.getInventory().setSelectedSlot(slot);
        ((cursed.recode.mixin.client.MultiPlayerGameModeAccess) mc.gameMode).cursed$sendCarried();
    }

    private static void swap(Minecraft mc, LocalPlayer p, int slot, int via) {
        int id = p.inventoryMenu.containerId;
        // (the inventory's slot in its menu: the hotbar is at the menu's end)
        int at = slot < 9 ? net.minecraft.world.inventory.InventoryMenu.USE_ROW_SLOT_START + slot : slot;
        mc.gameMode.handleContainerInput(id, at, via, ContainerInput.SWAP, p);
        if (mc.gui.screen() == null) {
            p.connection.send(new ServerboundContainerClosePacket(id));
        }
    }
}
