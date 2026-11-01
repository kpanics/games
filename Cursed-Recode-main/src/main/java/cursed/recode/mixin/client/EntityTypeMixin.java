package cursed.recode.mixin.client;

import cursed.recode.render.RendererSlot;
import net.minecraft.world.entity.EntityType;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Unique;

/** The type's renderer (cursed.recode.render.RendererSlot). */
@Mixin(EntityType.class)
public abstract class EntityTypeMixin implements RendererSlot {
    @Unique
    private Object cursed$renderer;
    @Unique
    private int cursed$generation;

    @Override
    public Object cursed$renderer() {
        return this.cursed$renderer;
    }

    @Override
    public int cursed$generation() {
        return this.cursed$generation;
    }

    @Override
    public void cursed$keep(Object renderer, int generation) {
        this.cursed$renderer = renderer;
        this.cursed$generation = generation;
    }
}
