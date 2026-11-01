package cursed.recode.swap;

import java.util.List;
import net.minecraft.client.Minecraft;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.component.DataComponents;
import net.minecraft.network.protocol.game.ServerboundContainerClosePacket;
import net.minecraft.network.protocol.game.ServerboundPlayerActionPacket;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.entity.EquipmentSlot;
import net.minecraft.world.inventory.ContainerInput;
import net.minecraft.world.inventory.InventoryMenu;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.enchantment.EnchantmentEffectComponents;
import net.minecraft.world.item.enchantment.EnchantmentHelper;
import net.minecraft.world.item.equipment.Equippable;
import org.jspecify.annotations.Nullable;

/**
 * A thing from the inventory put on its place, the thing there going where it came from (the inventory stays as it
 * was laid out). From the whole inventory: the game's own clicks on the player's inventory, the inventory closed —
 * the swap keys' presses (a swap with the off hand is one, armour from the hotbar one, from above it three through a
 * hotbar slot, which is left as it was) or three left clicks carrying the thing (taken, put on, what was there put
 * where it was), as the module's timing says; AutoSwap does them at its ticks, then the inventory is told closed if
 * asked. From the hotbar by hand: what a player does — the slot picked, F for the off hand, the right button for
 * armour and elytra (the game puts them on from the hand) — and the slot held before picked back, all in one tick (a
 * head is clicked: the game does not put it on from the hand).
 */
final class Moves {
    private Moves() {
    }

    /**
     * Where things may be taken from, and how: anywhere, by the inventory's clicks; only the hotbar, by hand (ReallyWorld,
     * the module's setting); FunTime's rule — the hotbar's by hand at once, from above it only totems (their talismans
     * are totems) and spheres, by the inventory's clicks after the pause.
     */
    enum Reach {
        ALL, HOTBAR, FUNTIME;

        /** Whether the thing in the inventory's slot `i` (0..35) may be taken. */
        boolean takes(int i, ItemStack st) {
            return i < 9 || this == ALL || this == FUNTIME && (st.is(Items.TOTEM_OF_UNDYING) || st.is(Items.PLAYER_HEAD));
        }

        /** Whether it is taken from slot `i` as a player does by hand (else by the inventory's clicks). */
        boolean byHand(int i) {
            return this == HOTBAR || this == FUNTIME && i < 9;
        }

        /** Whether taking from slot `i` waits the server's pause (FunTime's, for the inventory's). */
        boolean slow(int i) {
            return this == FUNTIME && i >= 9;
        }
    }

    /** The places' slots in the player's inventory menu (gui/swap.rs PLACES). */
    static final EquipmentSlot[] SLOTS = {EquipmentSlot.OFFHAND, EquipmentSlot.HEAD, EquipmentSlot.CHEST, EquipmentSlot.LEGS, EquipmentSlot.FEET};
    private static final int[] MENU = {InventoryMenu.SHIELD_SLOT, 5, 6, 7, 8};

    /** The inventory's slot (0..35) in the inventory menu: the hotbar is at its end there. */
    static int menuSlot(int inv) {
        return inv < 9 ? InventoryMenu.USE_ROW_SLOT_START + inv : inv;
    }

    /** What is on the place now. */
    static ItemStack worn(LocalPlayer p, int place) {
        return p.getItemBySlot(SLOTS[place]);
    }

    /** Whether putting the thing in slot `from` on `place` takes the inventory's clicks (else it goes by hand: a head
     *  from the hotbar is clicked too, the game does not put it on from the hand). */
    static boolean byClicks(LocalPlayer p, int place, int from, Reach reach) {
        if (!reach.byHand(from)) {
            return true;
        }
        Equippable e = p.getInventory().getItem(from).get(DataComponents.EQUIPPABLE);
        return place > 0 && (e == null || !e.swappable());
    }

    /** Whether the place may be changed now: no other container open, not a cursed piece of armour that will not come
     *  off (but in creative). */
    static boolean free(Minecraft mc, LocalPlayer p, int place) {
        if (p.containerMenu != p.inventoryMenu || p.isSpectator()) {
            return false;
        }
        ItemStack on = worn(p, place);
        return place == 0 || p.isCreative() || on.isEmpty() || !EnchantmentHelper.has(on, EnchantmentEffectComponents.PREVENT_ARMOR_CHANGE);
    }

    /** A click in the player's inventory: the menu's slot, the button (a swap key's: the hotbar's number, 40 the off
     *  hand), the kind (a left click carries what it takes). */
    record Click(int slot, int button, ContainerInput kind) {
    }

    /** The clicks that put the thing in the inventory's slot `from` (0..35) on `place`: three left clicks carrying it
     *  (`three`), else the swap keys' presses; none when it goes by hand (byHand); null when it may not be done now. */
    static @Nullable List<Click> clicks(Minecraft mc, LocalPlayer p, int place, int from, Reach reach, boolean three) {
        if (mc.gameMode == null || !free(mc, p, place) || from < 0 || from > 35 || !reach.takes(from, p.getInventory().getItem(from))) {
            return null;
        }
        if (!byClicks(p, place, from, reach)) {
            return List.of();
        }
        int at = menuSlot(from);
        // (a stack onto armour by the swap keys: a left click does not put more than one on it, the key splits it)
        if (three && (place == 0 || p.getInventory().getItem(from).getCount() == 1)) {
            return List.of(new Click(at, 0, ContainerInput.PICKUP), new Click(MENU[place], 0, ContainerInput.PICKUP), new Click(at, 0, ContainerInput.PICKUP));
        }
        if (place == 0) {
            return List.of(new Click(at, 40, ContainerInput.SWAP));
        }
        if (from < 9) {
            return List.of(new Click(MENU[place], from, ContainerInput.SWAP));
        }
        // through a hotbar slot (an empty one if there is, else the one held): there and back, it stays as it was
        int via = p.getInventory().getSelectedSlot();
        for (int i = 0; i < 9; i++) {
            if (p.getInventory().getItem(i).isEmpty()) {
                via = i;
                break;
            }
        }
        return List.of(new Click(at, via, ContainerInput.SWAP), new Click(MENU[place], via, ContainerInput.SWAP), new Click(at, via, ContainerInput.SWAP));
    }

    /** Click `n` of a swap's (`c`): false when it is not done — the inventory is not the player's any more, or a left
     *  click finds the cursor not as the clicks before left it (the first takes with nothing carried, the rest put
     *  what is carried: one with nothing carried would take what is there). */
    static boolean click(Minecraft mc, LocalPlayer p, Click c, int n) {
        if (mc.gameMode == null || p.containerMenu != p.inventoryMenu) {
            return false;
        }
        if (c.kind() == ContainerInput.PICKUP && p.inventoryMenu.getCarried().isEmpty() != (n == 0)) {
            return false;
        }
        mc.gameMode.handleContainerInput(p.inventoryMenu.containerId, c.slot(), c.button(), c.kind(), p);
        return true;
    }

    /** Whether a swap by left clicks is over after its click `n`: what it carried put where nothing was. */
    static boolean settled(LocalPlayer p, Click c, int n) {
        return c.kind() == ContainerInput.PICKUP && n > 0 && p.inventoryMenu.getCarried().isEmpty();
    }

    /** A thing left carried by a swap's left clicks (one cut short: a menu opened between them, a thing picked up
     *  into the slot emptied) put back, once the player's inventory is the menu again and no screen is open: into
     *  `home` (the menu's slot it came from) if that is empty, else the first empty slot, else the inventory told
     *  closed (the server puts it in or drops it). True when nothing is left carried. */
    static boolean giveBack(Minecraft mc, LocalPlayer p, int home) {
        if (p.inventoryMenu.getCarried().isEmpty()) {
            return true;
        }
        if (mc.gameMode == null || p.containerMenu != p.inventoryMenu || mc.gui.screen() != null) {
            return false;
        }
        int to = home >= InventoryMenu.INV_SLOT_START && home < InventoryMenu.USE_ROW_SLOT_END && !p.inventoryMenu.getSlot(home).hasItem() ? home : -1;
        for (int i = InventoryMenu.INV_SLOT_START; i < InventoryMenu.USE_ROW_SLOT_END && to < 0; i++) {
            if (!p.inventoryMenu.getSlot(i).hasItem()) {
                to = i;
            }
        }
        if (to >= 0) {
            mc.gameMode.handleContainerInput(p.inventoryMenu.containerId, to, 0, ContainerInput.PICKUP, p);
        } else {
            p.connection.send(new ServerboundContainerClosePacket(p.inventoryMenu.containerId));
        }
        return true;
    }

    /** The inventory told closed (as a player's closing it): only when no screen is open. */
    static void close(Minecraft mc, LocalPlayer p) {
        if (mc.gui.screen() == null) {
            p.connection.send(new ServerboundContainerClosePacket(p.inventoryMenu.containerId));
        }
    }

    /** As a player does it by hand, from the hotbar's slot `from` (clicks says when: not a head). */
    static boolean byHand(Minecraft mc, LocalPlayer p, int place, int from) {
        if (mc.gameMode == null) {
            return false;
        }
        int held = p.getInventory().getSelectedSlot();
        var access = (cursed.recode.mixin.client.MultiPlayerGameModeAccess) mc.gameMode;
        p.getInventory().setSelectedSlot(from);
        access.cursed$sendCarried();
        if (place == 0) {
            p.connection.send(new ServerboundPlayerActionPacket(ServerboundPlayerActionPacket.Action.SWAP_ITEM_WITH_OFFHAND, BlockPos.ZERO, Direction.DOWN));
        } else {
            mc.gameMode.useItem(p, InteractionHand.MAIN_HAND);
        }
        p.getInventory().setSelectedSlot(held);
        access.cursed$sendCarried();
        return true;
    }
}
