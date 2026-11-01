package cursed.recode.swap;

import java.util.ArrayList;
import java.util.List;
import net.minecraft.core.Holder;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.entity.EquipmentSlot;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.component.ItemAttributeModifiers;
import net.minecraft.world.item.enchantment.Enchantment;
import net.minecraft.world.item.equipment.Equippable;

/**
 * What a swap's list asks for (native gui/swap.rs KINDS, or a thing picked: its id, name and enchantments), whether
 * a stack is it and may go where the swap puts it, and how good it is among those that are (the best goes first).
 */
final class Things {
    private Things() {
    }

    /** One thing of a swap's list: picked (id, name, enchantments) or a kind (kind not null). */
    record Pick(String id, String name, String ench, String kind) {
    }

    static String id(ItemStack st) {
        return BuiltInRegistries.ITEM.getKey(st.getItem()).toString();
    }

    static String name(ItemStack st) {
        return st.getHoverName().getString();
    }

    /** Its enchantments as "id:level", by id, joined by commas. */
    static String ench(ItemStack st) {
        List<String> out = new ArrayList<>();
        for (var e : st.getEnchantments().entrySet()) {
            Holder<Enchantment> h = e.getKey();
            out.add(h.unwrapKey().map(k -> k.identifier().toString()).orElse("?") + ":" + e.getIntValue());
        }
        out.sort(null);
        return String.join(",", out);
    }

    /** The same thing: its id, name and enchantments (not how worn it is or how many). */
    static boolean same(ItemStack a, ItemStack b) {
        return !a.isEmpty() && !b.isEmpty() && a.getItem() == b.getItem() && name(a).equals(name(b)) && ench(a).equals(ench(b));
    }

    /** Whether `st` may go to `slot` at all (the off hand takes anything; armour's places what is worn there). */
    static boolean goes(ItemStack st, EquipmentSlot slot) {
        if (slot == EquipmentSlot.OFFHAND) {
            return true;
        }
        Equippable e = st.get(DataComponents.EQUIPPABLE);
        return e != null && e.slot() == slot;
    }

    /** Whether `st` is what `p` asks for, to be put on `slot` (`worn` what is there now). */
    static boolean fits(Pick p, ItemStack st, ItemStack worn, EquipmentSlot slot) {
        if (st.isEmpty() || !goes(st, slot)) {
            return false;
        }
        if (p.kind() == null) {
            return id(st).equals(p.id()) && name(st).equals(p.name()) && ench(st).equals(p.ench());
        }
        return switch (p.kind()) {
            case "totem" -> st.is(Items.TOTEM_OF_UNDYING);
            // a server's own totem: enchanted or named
            case "talisman" -> st.is(Items.TOTEM_OF_UNDYING) && (st.isEnchanted() || st.getCustomName() != null);
            case "shield" -> st.is(Items.SHIELD);
            case "sphere" -> st.is(Items.PLAYER_HEAD);
            case "firework" -> st.is(Items.FIREWORK_ROCKET);
            case "golden_apple" -> st.is(Items.GOLDEN_APPLE);
            case "enchanted_apple" -> st.is(Items.ENCHANTED_GOLDEN_APPLE);
            case "food" -> st.has(DataComponents.FOOD);
            case "elytra" -> st.has(DataComponents.GLIDER);
            case "helmet" -> armour(st, EquipmentSlot.HEAD);
            case "chestplate" -> armour(st, EquipmentSlot.CHEST) && !st.has(DataComponents.GLIDER);
            case "leggings" -> armour(st, EquipmentSlot.LEGS);
            case "boots" -> armour(st, EquipmentSlot.FEET);
            // another of what is worn (the armour's own replacement): the same thing, other wear
            case "same" -> same(st, worn);
            default -> false;
        };
    }

    /** Armour for `slot`: worn there and giving armour (not a head, not a pumpkin). */
    private static boolean armour(ItemStack st, EquipmentSlot slot) {
        Equippable e = st.get(DataComponents.EQUIPPABLE);
        return e != null && e.slot() == slot && armourPoints(st) > 0.0;
    }

    static double armourPoints(ItemStack st) {
        double sum = 0.0;
        for (ItemAttributeModifiers.Entry e : st.getOrDefault(DataComponents.ATTRIBUTE_MODIFIERS, ItemAttributeModifiers.EMPTY).modifiers()) {
            if (e.attribute().equals(Attributes.ARMOR) || e.attribute().equals(Attributes.ARMOR_TOUGHNESS)) {
                sum += e.modifier().amount();
            }
        }
        return sum;
    }

    /** How much of it is left (1 new, 0 about to break; 1 when it does not wear). */
    static double left(ItemStack st) {
        return st.isDamageableItem() && st.getMaxDamage() > 0 ? 1.0 - st.getDamageValue() / (double) st.getMaxDamage() : 1.0;
    }

    /** How good it is among those that fit: more armour, then more of it left, then a bigger stack. */
    static double worth(ItemStack st) {
        return armourPoints(st) * 1000.0 + left(st) * 100.0 + st.getCount() / 64.0;
    }
}
