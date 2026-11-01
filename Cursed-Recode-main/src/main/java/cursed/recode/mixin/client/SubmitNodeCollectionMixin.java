package cursed.recode.mixin.client;

import cursed.recode.scene.TextCache;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.Font;
import net.minecraft.client.renderer.SubmitNodeCollection;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.FormattedText;
import net.minecraft.util.FormattedCharSequence;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Redirect;

/** A name tag's width and visual order laid out once per name, not every frame (cursed.recode.scene.TextCache). */
@Mixin(SubmitNodeCollection.class)
public abstract class SubmitNodeCollectionMixin {
    @Redirect(method = "submitNameTag", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/gui/Font;width(Lnet/minecraft/network/chat/FormattedText;)I"))
    private int cursed$width(Font font, FormattedText text) {
        return text instanceof Component c ? TextCache.name(c, font).width() : font.width(text);
    }

    @Redirect(method = "submitNameTag", at = @At(value = "INVOKE", target = "Lnet/minecraft/network/chat/Component;getVisualOrderText()Lnet/minecraft/util/FormattedCharSequence;"))
    private FormattedCharSequence cursed$order(Component name) {
        return TextCache.name(name, Minecraft.getInstance().font).sequence();
    }

    // Other Custom Things → Holograms (cursed.recode.hud.HudOther): the name tags' plate, shadow, size, fading with distance

    @org.spongepowered.asm.mixin.injection.Inject(method = "submitNameTag", at = @At("HEAD"))
    private void cursed$tag(com.mojang.blaze3d.vertex.PoseStack poseStack, net.minecraft.world.phys.Vec3 at, int offset, Component name, boolean seeThrough,
                            int light, net.minecraft.client.renderer.state.level.CameraRenderState camera,
                            org.spongepowered.asm.mixin.injection.callback.CallbackInfo ci) {
        cursed.recode.hud.HudOther.tag(poseStack.last().pose());
    }

    @Redirect(method = "submitNameTag", at = @At(value = "INVOKE", target = "Lcom/mojang/blaze3d/vertex/PoseStack;scale(FFF)V"))
    private void cursed$tagScale(com.mojang.blaze3d.vertex.PoseStack pose, float x, float y, float z) {
        float k = cursed.recode.hud.HudOther.scale();
        pose.scale(x * k, y * k, z * k);
    }

    @Redirect(method = "submitNameTag", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/renderer/state/OptionsRenderState;getBackgroundOpacity(F)F"))
    private float cursed$tagOpacity(net.minecraft.client.renderer.state.OptionsRenderState options, float dflt) {
        return cursed.recode.hud.HudOther.opacity(options.getBackgroundOpacity(dflt));
    }

    @org.spongepowered.asm.mixin.injection.ModifyArg(method = "submitNameTag", index = 5, at = @At(value = "INVOKE",
            target = "Lnet/minecraft/client/renderer/SubmitNodeCollection;nameTag(Lorg/joml/Matrix4f;FFLnet/minecraft/util/FormattedCharSequence;IIILnet/minecraft/client/gui/Font$DisplayMode;)Lnet/minecraft/client/renderer/feature/TextFeatureRenderer$Submit;"))
    private int cursed$tagWords(int color) {
        return cursed.recode.hud.HudOther.words(color);
    }

    @org.spongepowered.asm.mixin.injection.ModifyArg(method = "submitNameTag", index = 6, at = @At(value = "INVOKE",
            target = "Lnet/minecraft/client/renderer/SubmitNodeCollection;nameTag(Lorg/joml/Matrix4f;FFLnet/minecraft/util/FormattedCharSequence;IIILnet/minecraft/client/gui/Font$DisplayMode;)Lnet/minecraft/client/renderer/feature/TextFeatureRenderer$Submit;"))
    private int cursed$tagPlate(int color) {
        return cursed.recode.hud.HudOther.plate(color);
    }

    @org.spongepowered.asm.mixin.injection.ModifyArg(method = "nameTag", index = 3, at = @At(value = "INVOKE",
            target = "Lnet/minecraft/client/renderer/feature/TextFeatureRenderer$Content$Text;<init>(FFLnet/minecraft/util/FormattedCharSequence;ZIII)V"))
    private static boolean cursed$tagShadow(boolean shadow) {
        return cursed.recode.hud.HudOther.shadow(shadow);
    }
}
