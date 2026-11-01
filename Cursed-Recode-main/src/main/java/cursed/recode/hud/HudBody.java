package cursed.recode.hud;

import java.util.List;
import net.minecraft.client.Minecraft;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.tags.FluidTags;
import net.minecraft.world.effect.MobEffects;
import net.minecraft.world.entity.HumanoidArm;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.GameType;

/**
 * The player's body for the Custom Hotbar (Interface; native gui/hudbar.rs): the hotbar's slots (each its count, how
 * worn, its cooldown), the one held, the off hand; health with absorption and its kind (poisoned, withered, frozen),
 * food and saturation, armour, air, experience, the mount's health and jump; the attack's strength; the held item's
 * name and since when; since when hurt. Of the game's own hotbar and bars, each part the Custom Hotbar draws itself is
 * left out; a part switched off is the game's again, lifted over the client's hotbar (her word, 25.10: "хпшки кастомные
 * не выключаются").
 */
public final class HudBody {
    private HudBody() {
    }

    /** How many numbers the body is (gui/hud.rs Body::parse). */
    public static final int SIZE = 55;
    /** The Custom Hotbar is on (the HUD says so each frame). */
    static boolean active;
    /** Its parts the HUD draws itself (native gui/mod.rs hud_state 82..91), by these numbers. */
    public static final int HOTBAR = 0, HEALTH = 1, FOOD = 2, ARMOUR = 3, AIR = 4, XP = 5, LEVEL = 6, MOUNT = 7, JUMP = 8, NAME = 9;
    private static final boolean[] DRAWN = new boolean[10];
    /** How high the client's hotbar stands: a part of the screen's height from its foot (0: not drawn). */
    private static float top;
    private static final ItemStack[] SLOTS = new ItemStack[10];
    private static int lastSel = -1;
    private static ItemStack lastHeld = ItemStack.EMPTY;
    private static long nameAt, hurtAt;
    private static float lastHealth = -1.0F;

    public static boolean active() {
        Minecraft mc = Minecraft.getInstance();
        return active && mc.gameMode != null && mc.gameMode.getPlayerMode() != GameType.SPECTATOR;
    }

    /** The HUD's word of this frame (HudLayer): which parts it draws, how high its hotbar stands. */
    static void state(float[] s) {
        for (int i = 0; i < DRAWN.length; i++) {
            DRAWN[i] = s[82 + i] > 0.5F;
        }
        top = s[92];
    }

    /** Whether the part `part` is the client's now (the game's own of it is not drawn). */
    public static boolean drawn(int part) {
        return active() && DRAWN[part];
    }

    /** How far (gui px) the game's bars given back go up to rest over the client's hotbar (0: its own place). */
    public static float lift(int guiHeight) {
        if (!drawn(HOTBAR)) {
            return 0.0F;
        }
        // (the game's hotbar is 22 high: its bars sit on it; on ours, a breath over its top)
        return Math.max(0.0F, top * guiHeight - 22.0F + 4.0F);
    }

    /** The body's numbers onto `n` (and the held item's name onto `text`). */
    static void append(Minecraft mc, LocalPlayer p, float partial, List<Double> n, StringBuilder text) {
        long now = System.nanoTime();
        var inv = p.getInventory();
        int sel = inv.getSelectedSlot();
        ItemStack held = inv.getItem(sel);
        if (sel != lastSel || !ItemStack.isSameItemSameComponents(held, lastHeld)) {
            if (!held.isEmpty()) {
                nameAt = now;
            }
            lastSel = sel;
            lastHeld = held.copy();
        }
        float hp = p.getHealth();
        if (lastHealth >= 0.0F && hp < lastHealth - 0.01F) {
            hurtAt = now;
        }
        lastHealth = hp;
        int heart = p.hasEffect(MobEffects.WITHER) ? 2 : p.hasEffect(MobEffects.POISON) ? 1 : p.isFullyFrozen() ? 3 : 0;
        var food = p.getFoodData();
        LivingEntity mount = p.getVehicle() instanceof LivingEntity le && le.showVehicleHealth() ? le : null;
        float jump = -1.0F;
        if (p.jumpableVehicle() != null) {
            jump = p.getJumpRidingScale();
        }
        n.add((double) sel);
        n.add(p.getMainArm().getOpposite() == HumanoidArm.LEFT ? 1.0 : 0.0);
        n.add((double) hp);
        n.add((double) Math.max((float) p.getAttributeValue(Attributes.MAX_HEALTH), hp));
        n.add((double) p.getAbsorptionAmount());
        n.add((double) heart);
        n.add(p.level().getLevelData().isHardcore() ? 1.0 : 0.0);
        n.add(p.hasEffect(MobEffects.REGENERATION) ? 1.0 : 0.0);
        n.add((double) food.getFoodLevel());
        n.add((double) food.getSaturationLevel());
        n.add(p.hasEffect(MobEffects.HUNGER) ? 1.0 : 0.0);
        n.add((double) p.getArmorValue());
        n.add(p.getAttributeValue(Attributes.ARMOR_TOUGHNESS));
        n.add((double) Math.max(0, p.getAirSupply()));
        n.add((double) p.getMaxAirSupply());
        n.add(p.isEyeInFluid(FluidTags.WATER) ? 1.0 : 0.0);
        n.add((double) p.experienceLevel);
        n.add((double) p.experienceProgress);
        n.add(mount != null ? (double) mount.getHealth() : 0.0);
        n.add(mount != null ? (double) mount.getMaxHealth() : 0.0);
        n.add((double) jump);
        n.add((double) p.getAttackStrengthScale(partial));
        n.add(mc.gameMode == null ? 0.0 : (double) mc.gameMode.getPlayerMode().getId());
        n.add(nameAt == 0 ? 99.0 : (now - nameAt) / 1e9);
        n.add(hurtAt == 0 ? 99.0 : (now - hurtAt) / 1e9);
        var cd = p.getCooldowns();
        for (int i = 0; i < 10; i++) {
            ItemStack s = i < 9 ? inv.getItem(i) : p.getOffhandItem();
            SLOTS[i] = s;
            n.add(s.isEmpty() ? 0.0 : (double) s.getCount());
            n.add(s.isDamageableItem() && s.isDamaged() ? 1.0 - (double) s.getDamageValue() / s.getMaxDamage() : -1.0);
            n.add(s.isEmpty() ? 0.0 : (double) cd.getCooldownPercent(s, partial));
        }
        String name = held.isEmpty() ? "" : held.getHoverName().getString();
        text.append(name.replace('\n', ' ')).append('\n');
    }

    /** A slot's item where the HUD left room for it (the icons of kind 9: 0..8 the hotbar, 9 the off hand). */
    static ItemStack slot(int i) {
        return i >= 0 && i < SLOTS.length && SLOTS[i] != null ? SLOTS[i] : ItemStack.EMPTY;
    }
}
