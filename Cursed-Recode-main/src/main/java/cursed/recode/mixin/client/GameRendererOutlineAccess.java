package cursed.recode.mixin.client;

import net.minecraft.client.renderer.GameRenderer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Invoker;

/** The game's own test of whether the block looked at is outlined (F1, adventure, spectator): Block Outline asks it. */
@Mixin(GameRenderer.class)
public interface GameRendererOutlineAccess {
    @Invoker("shouldRenderBlockOutline")
    boolean cursed$shouldRenderBlockOutline();
}
