package cursed.recode.mixin.client;

import cursed.recode.world.StateId;
import net.minecraft.world.level.block.state.BlockState;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Unique;

/** A state's number in Block.BLOCK_STATE_REGISTRY, for the native mesher (set by BlockExport). */
@Mixin(BlockState.class)
public abstract class BlockStateMixin implements StateId {
    @Unique
    private int cursed$id;

    @Override
    public int cursed$id() {
        return this.cursed$id;
    }

    @Override
    public void cursed$setId(int id) {
        this.cursed$id = id;
    }
}
