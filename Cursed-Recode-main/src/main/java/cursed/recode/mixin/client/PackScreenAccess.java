package cursed.recode.mixin.client;

import net.minecraft.client.gui.components.EditBox;
import net.minecraft.client.gui.screens.inventory.AbstractMountInventoryScreen;
import net.minecraft.client.gui.screens.inventory.AnvilScreen;
import net.minecraft.world.entity.LivingEntity;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

/** Custom Inventory (cursed.recode.hud.HudPack): what a container screen keeps to itself that its blocks show. */
public final class PackScreenAccess {
    private PackScreenAccess() {
    }

    /** The anvil's name field (its words, caret and selection drawn by the HUD, its box moved to the HUD's field). */
    @Mixin(AnvilScreen.class)
    public interface Anvil {
        @Accessor("name")
        EditBox cursed$name();
    }

    /** The enchanting table's book, drawn where the HUD's block keeps its place. */
    @Mixin(net.minecraft.client.gui.screens.inventory.EnchantmentScreen.class)
    public interface Enchant {
        @org.spongepowered.asm.mixin.gen.Invoker("extractBook")
        void cursed$book(net.minecraft.client.gui.GuiGraphicsExtractor graphics, int left, int top);
    }

    /** The stonecutter's first recipe shown and its scroll. */
    @Mixin(net.minecraft.client.gui.screens.inventory.StonecutterScreen.class)
    public interface Cutter {
        @Accessor("startIndex")
        int cursed$start();

        @Accessor("scrollOffs")
        float cursed$scroll();
    }

    /** A trader's first offer shown and the chosen one. */
    @Mixin(net.minecraft.client.gui.screens.inventory.MerchantScreen.class)
    public interface Trader {
        @Accessor("scrollOff")
        int cursed$scroll();

        @Accessor("shopItem")
        int cursed$chosen();
    }

    /** A loom's patterns: its first row shown, scroll, whether they show, the banner to come, the flag it is drawn on. */
    @Mixin(net.minecraft.client.gui.screens.inventory.LoomScreen.class)
    public interface Loom {
        @Accessor("startRow")
        int cursed$startRow();

        @Accessor("scrollOffs")
        float cursed$scroll();

        @Accessor("displayPatterns")
        boolean cursed$display();

        @Accessor("hasMaxPatterns")
        boolean cursed$max();

        @Accessor("resultBannerPatterns")
        net.minecraft.world.level.block.entity.BannerPatternLayers cursed$result();

        @Accessor("flag")
        net.minecraft.client.model.object.banner.BannerFlagModel cursed$flag();

        @Accessor("dyeStack")
        net.minecraft.world.item.ItemStack cursed$dye();
    }

    /** A beacon's chosen powers. */
    @Mixin(net.minecraft.client.gui.screens.inventory.BeaconScreen.class)
    public interface Beacon {
        @Accessor("primary")
        net.minecraft.core.Holder<net.minecraft.world.effect.MobEffect> cursed$primary();

        @Accessor("secondary")
        net.minecraft.core.Holder<net.minecraft.world.effect.MobEffect> cursed$secondary();
    }

    /** The horse, llama, camel or nautilus shown beside its slots. */
    @Mixin(AbstractMountInventoryScreen.class)
    public interface Mount {
        @Accessor("mount")
        LivingEntity cursed$mount();
    }
}
