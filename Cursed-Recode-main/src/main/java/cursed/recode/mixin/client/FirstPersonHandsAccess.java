package cursed.recode.mixin.client;

import com.mojang.blaze3d.vertex.PoseStack;
import net.minecraft.client.renderer.FirstPersonHandsAndItemsRenderer;
import net.minecraft.world.entity.HumanoidArm;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Invoker;

/** The game's own swing of a hand (FirstPersonHandsMixin draws it when no style of ours is chosen). */
@Mixin(FirstPersonHandsAndItemsRenderer.class)
public interface FirstPersonHandsAccess {
    @Invoker("swingArm")
    void cursed$swingArm(float animation, PoseStack poseStack, int invert, HumanoidArm arm);
}
