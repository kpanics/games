package cursed.recode.mixin.client;

import cursed.recode.scene.NameMemo;
import net.minecraft.network.chat.Component;
import net.minecraft.world.entity.LivingEntity;
import org.jspecify.annotations.Nullable;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Unique;

/** The answer for the custom name's upside-down check, kept with the name it was for (cursed.recode.scene.NameMemo). */
@Mixin(LivingEntity.class)
public abstract class LivingEntityMixin implements NameMemo {
    @Unique
    private @Nullable Component cursed$upsideName;
    @Unique
    private boolean cursed$upside;

    @Override
    public @Nullable Component cursed$upsideName() {
        return this.cursed$upsideName;
    }

    @Override
    public boolean cursed$upside() {
        return this.cursed$upside;
    }

    @Override
    public void cursed$upside(Component name, boolean upside) {
        this.cursed$upsideName = name;
        this.cursed$upside = upside;
    }
}
