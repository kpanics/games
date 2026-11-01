package cursed.recode.fx;

import com.mojang.blaze3d.vertex.PoseStack;
import com.mojang.math.Axis;
import cursed.recode.render.Native;
import net.minecraft.core.component.DataComponents;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.item.BlockItem;
import net.minecraft.world.item.BowItem;
import net.minecraft.world.item.CrossbowItem;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;

/**
 * View Model and Swing Animations (native gui/viewmodel.rs decides): a hand drawn in the first person is asked about
 * once (FirstPersonHandsMixin): where it holds its item (put on right after the game's own place for the hand) and,
 * when a style of ours is chosen, where its swing is now (instead of the game's swing).
 */
public final class ViewModel {
    /** This hand's answer: View Model's pose, the swing's (none: the game's own), the swing's editor's (the hand held
     *  where the swing being made is, with or without a swing of the game's). */
    private static float[] pose;
    private static float[] swing;
    private static float[] edit;
    private static float[][] operations;
    private static int classicSource;
    private static boolean classicApplied;
    private static float direction, equip, equipFactor = Float.NaN;

    private ViewModel() {
    }

    /** What kind of item, as View Model's choice names them. */
    static String kind(ItemStack s) {
        if (s.isEmpty()) {
            return "Other";
        }
        if (s.getItem() instanceof BowItem || s.getItem() instanceof CrossbowItem) {
            return "Bows";
        }
        if (s.has(DataComponents.WEAPON) || s.is(Items.TRIDENT) || s.is(Items.MACE)) {
            return "Weapons";
        }
        if (s.has(DataComponents.FOOD) || s.has(DataComponents.CONSUMABLE) || s.has(DataComponents.POTION_CONTENTS)) {
            return "Food";
        }
        if (s.has(DataComponents.TOOL)) {
            return "Tools";
        }
        if (s.getItem() instanceof BlockItem) {
            return "Blocks";
        }
        return "Other";
    }

    private static float[] floats(String line) {
        String[] f = line.trim().split(" ");
        float[] v = new float[f.length - 1];
        try {
            for (int i = 1; i < f.length; i++) {
                v[i - 1] = Float.parseFloat(f[i]);
            }
        } catch (NumberFormatException e) {
            return null;
        }
        return v.length >= 7 ? v : null;
    }

    /** A hand about to be drawn (its item, how far into its swing): its answer, asked now. */
    public static void begin(InteractionHand hand, ItemStack stack, float attack, float inverseArmHeight, float partialTicks) {
        pose = null;
        swing = null;
        edit = null;
        operations = null;
        classicSource = 0;
        classicApplied = false;
        equip = inverseArmHeight;
        equipFactor = Float.NaN;
        var player = net.minecraft.client.Minecraft.getInstance().player;
        var arm = player == null ? net.minecraft.world.entity.HumanoidArm.RIGHT : player.getMainArm();
        if (hand == InteractionHand.OFF_HAND) arm = arm.getOpposite();
        direction = arm == net.minecraft.world.entity.HumanoidArm.RIGHT ? 1.0F : -1.0F;
        int itemKind = stack.is(net.minecraft.tags.ItemTags.SWORDS) ? 0
                : stack.is(net.minecraft.tags.ItemTags.AXES) ? 1 : stack.is(net.minecraft.tags.ItemTags.SHOVELS) ? 2
                : stack.getUseAnimation() == net.minecraft.world.item.ItemUseAnimation.SPEAR ? 4
                : stack.getUseAnimation() == net.minecraft.world.item.ItemUseAnimation.BLOCK ? 5
                : stack.has(DataComponents.TOOL) ? 3 : 6;
        boolean eligible = player != null && !player.isUsingItem() && !player.isAutoSpinAttack()
                && !(stack.getItem() instanceof CrossbowItem) && !stack.has(DataComponents.MAP_ID);
        String said = Native.guiCall("hand\t" + (hand == InteractionHand.MAIN_HAND ? "main" : "off") + "\t" + kind(stack) + "\t" + attack
                + "\t" + (stack.isEmpty() ? "1" : "0") + "\t" + (direction < 0 ? "left" : "right")
                + "\t" + inverseArmHeight + "\t" + itemKind + "\t" + (player != null && player.isSwinging() ? "1" : "0")
                + "\t" + (eligible ? "1" : "0") + "\t" + (player == null ? 0 : player.tickCount + partialTicks)
                + "\t" + System.currentTimeMillis() + "\t" + System.identityHashCode(net.minecraft.client.Minecraft.getInstance().level) + "\t" + (player != null && player.getSwingAnimation(1.0F) > 0.0F ? "1" : "0"));
        if (said == null || said.isEmpty()) {
            return;
        }
        for (String l : said.split("\n")) {
            if (l.startsWith("equip-factor ")) {
                try { equipFactor = Float.parseFloat(l.substring(13)); } catch (NumberFormatException ignored) { equipFactor = Float.NaN; }
            } else if (l.startsWith("classic ")) {
                try { classicSource = Integer.parseInt(l.substring(8)); } catch (NumberFormatException ignored) { classicSource = 0; }
            } else if (l.startsWith("ops ")) {
                operations = operations(l.substring(4));
            } else if (l.startsWith("vm ")) {
                pose = floats(l);
            } else if (l.startsWith("swing ")) {
                swing = floats(l);
            } else if (l.startsWith("edit ")) {
                edit = floats(l);
            }
        }
    }

    private static float[][] operations(String value) {
        if (value.isBlank()) return new float[0][];
        String[] entries = value.split(";");
        if (entries.length > 64) return null;
        float[][] result = new float[entries.length][];
        try {
            for (int i = 0; i < entries.length; i++) {
                String[] parts = entries[i].split(" ");
                int type = switch(parts[0]) { case "t" -> 0; case "r" -> 1; case "s" -> 2; default -> -1; };
                if (type < 0 || parts.length != (type == 0 ? 4 : type == 1 ? 5 : 2)) return null;
                result[i] = new float[parts.length]; result[i][0] = type;
                for (int j = 1; j < parts.length; j++) {
                    result[i][j] = Float.parseFloat(parts[j]);
                    if (!Float.isFinite(result[i][j])) return null;
                }
            }
        } catch (NumberFormatException e) { return null; }
        return result;
    }

    /** Original clients apply view-model offsets before their base hand placement. */
    public static void beforeBase(PoseStack p) {
        if (classicSource > 0 && operations != null && pose != null) put(p, pose);
    }

    public static boolean classicApplied() { return classicApplied; }

    private static void put(PoseStack p, float[] v) {
        p.translate(v[0], v[1], v[2]);
        p.rotateDegrees(Axis.XP, v[3]);
        p.rotateDegrees(Axis.YP, v[4]);
        p.rotateDegrees(Axis.ZP, v[5]);
        p.scale(v[6], v[6], v[6]);
    }

    /** Right after the game's place for the hand: View Model's pose (and the swing's editor's, there even with no swing). */
    public static void pose(PoseStack p) {
        if (Float.isFinite(equipFactor)) p.translate(0.0F, equip * (0.6F - Math.clamp(equipFactor, 0.0F, 1.0F)), 0.0F);
        if (classicSource > 0 && operations != null) {
            if (classicSource == 1) p.translate(-direction * 0.56F, 0.52F + equip * 0.6F, 0.72F);
            for (float[] op : operations) {
                switch ((int) op[0]) {
                    case 0 -> p.translate(op[1], op[2], op[3]);
                    case 1 -> p.rotate(new org.joml.Quaternionf().rotationAxis((float) Math.toRadians(op[4]), op[1], op[2], op[3]));
                    case 2 -> p.scale(op[1], op[1], op[1]);
                    default -> { }
                }
            }
            classicApplied = true;
        } else if (pose != null) {
            put(p, pose);
        }
        if (edit != null) {
            put(p, edit);
        }
    }

    /** The swing: ours when a style of ours is chosen (true), else the game's is drawn (none while the editor holds it). */
    public static boolean swing(PoseStack p) {
        if (classicApplied || edit != null) {
            return true;
        }
        if (swing == null) {
            return false;
        }
        put(p, swing);
        return true;
    }

    private static java.util.Map<String, String> set = java.util.Map.of();
    private static long polled;

    /** How long the local player's swing lasts (ticks), as Swing Animations' speed says (only the look of it). */
    public static int duration(int ticks) {
        long now = System.nanoTime();
        if (now - polled > 250_000_000L) {
            polled = now;
            set = Fx.settings("swing-animations");
        }
        if (!"1".equals(set.get("on"))) {
            return ticks;
        }
        String source = set.getOrDefault("Animation source", "Recode");
        if (source.equals("Catlavan") || source.equals("Newcode")) return Math.max(1, Math.min(20, Math.round(Fx.number(set, "Swing ticks", 11))));
        if (source.equals("Nurik")) return ticks;
        float speed = Fx.number(set, "Speed", 1.0F);
        return Math.max(1, Math.round(ticks / Math.max(0.1F, speed)));
    }
}
