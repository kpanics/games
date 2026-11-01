package cursed.recode.mixin.client;

import cursed.recode.render.NativeTerrain;
import net.minecraft.client.renderer.GameRenderer;
import org.joml.Matrix4f;
import net.minecraft.client.renderer.state.GameRenderState;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.ModifyArg;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** The level's projection matrix as the game hands it to its buffer, kept for terrain occlusion (NativeTerrain). */
@Mixin(GameRenderer.class)
public abstract class GameRendererMixin {
    @Shadow @Final private GameRenderState gameRenderState;
    @Shadow @Final private com.mojang.blaze3d.pipeline.RenderTarget mainRenderTarget;

    /** While the level is drawn, the game's main target is the engine's smaller one (cursed.recode.engine.Upscale). */
    @Inject(method = "mainRenderTarget", at = @At("HEAD"), cancellable = true)
    private void cursed$scaledTarget(org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable<com.mojang.blaze3d.pipeline.RenderTarget> cir) {
        com.mojang.blaze3d.pipeline.RenderTarget t = cursed.recode.engine.Upscale.target();
        if (t != null) {
            cir.setReturnValue(t);
        }
    }

    /** The level is drawn: its frames go together into the main target before the hand. */
    @Inject(method = "renderLevel", at = @At(value = "INVOKE",
            target = "Lnet/minecraft/client/renderer/GameRenderer;render3dHud(Lnet/minecraft/client/renderer/state/level/CameraRenderState;Lnet/minecraft/client/renderer/state/level/PlayerRenderState;Lnet/minecraft/client/renderer/state/OptionsRenderState;Z)V"))
    private void cursed$upscale(CallbackInfo ci) {
        // CS's item in the player's hand from behind, into the level's own picture
        cursed.recode.cosmetics.Cosmetics.thirdDraw(this.mainRenderTarget);
        cursed.recode.engine.Upscale.end(this.mainRenderTarget);
        // Chams' ink in the air, over the level and under the hand
        cursed.recode.chams.ChamsInk.flush(this.mainRenderTarget, cursed.recode.engine.Upscale.levelDepth(), this.gameRenderState.levelRenderState.cameraRenderState);
        // Target ESP over them
        cursed.recode.render.TargetEsp.flush(this.mainRenderTarget, cursed.recode.engine.Upscale.levelDepth(), this.gameRenderState.levelRenderState.cameraRenderState);
        cursed.recode.render.Hitboxes.flush(this.mainRenderTarget, cursed.recode.engine.Upscale.levelDepth(), this.gameRenderState.levelRenderState.cameraRenderState);
        // the world's effects last (Chunk Grid, …)
        cursed.recode.fx.Fx.flush(this.mainRenderTarget, cursed.recode.engine.Upscale.levelDepth(), this.gameRenderState.levelRenderState.cameraRenderState);
    }

    /** The engine's frame goes first: the voxel window and the sun for the terrain about to be drawn. */
    @Inject(method = "renderLevel", at = @At("HEAD"))
    private void cursed$engine(CallbackInfo ci) {
        cursed.recode.engine.Upscale.begin(this.mainRenderTarget);
        cursed.recode.engine.Engine.frame(this.gameRenderState.levelRenderState);
    }

    /** The client's menu, once the world is drawn and before the game's GUI: its glass blurs this frame's world. */
    @Inject(method = "render", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/gui/render/GuiRenderer;render()V"))
    private void cursed$menu(CallbackInfo ci) {
        cursed.recode.menus.WorldShot.take(this.mainRenderTarget);
        cursed.recode.render.GuiFrame.flush(this.mainRenderTarget.getColorTexture());
    }

    /** Cosmetics: the hands are ours (a CS knife for a sword…, native/src/cosmetics/): drawn by the engine, the
     *  game's own hand left out. */
    @Inject(method = "renderItemInHand", at = @At("HEAD"), cancellable = true)
    private void cursed$cosmeticHands(CallbackInfo ci) {
        if (cursed.recode.cosmetics.Cosmetics.hands(this.mainRenderTarget)) {
            ci.cancel();
        }
    }

    /** No Render: no shake of the view when hurt (cursed.recode.fx.NoRender). */
    @Inject(method = "bobHurt", at = @At("HEAD"), cancellable = true)
    private void cursed$noHurtShake(CallbackInfo ci) {
        if (cursed.recode.fx.NoRender.on("Hurt camera shake")) {
            ci.cancel();
        }
    }

    /** No Render: nausea's and a portal's warp of the world gone (their overlays: HudMixin). */
    @org.spongepowered.asm.mixin.injection.Redirect(method = "renderLevel", at = @At(value = "FIELD", opcode = org.objectweb.asm.Opcodes.GETFIELD,
            target = "Lnet/minecraft/client/renderer/state/level/PlayerRenderState;portalEffectIntensity:F"))
    private float cursed$portalWarp(net.minecraft.client.renderer.state.level.PlayerRenderState state) {
        return cursed.recode.fx.NoRender.on("Nausea and portal") ? 0.0F : state.portalEffectIntensity;
    }

    @org.spongepowered.asm.mixin.injection.Redirect(method = "renderLevel", at = @At(value = "FIELD", opcode = org.objectweb.asm.Opcodes.GETFIELD,
            target = "Lnet/minecraft/client/renderer/state/level/PlayerRenderState;nauseaEffectIntensity:F"))
    private float cursed$nauseaWarp(net.minecraft.client.renderer.state.level.PlayerRenderState state) {
        return cursed.recode.fx.NoRender.on("Nausea and portal") ? 0.0F : state.nauseaEffectIntensity;
    }

    /** Aspect Ratio: the hand drawn as on a narrower screen too (cursed.recode.fx.AspectRatio). */
    @ModifyArg(method = "render3dHud", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/renderer/Projection;setupPerspective(FFFFF)V"), index = 3)
    private float cursed$handWidth(float width) {
        return cursed.recode.fx.AspectRatio.handWidth(width);
    }

    @ModifyArg(method = "renderLevel", at = @At(value = "INVOKE",
            target = "Lnet/minecraft/client/renderer/ProjectionMatrixBuffer;getBuffer(Lorg/joml/Matrix4f;)Lcom/mojang/renderpearl/api/buffers/GpuBufferSlice;", ordinal = 0))
    private Matrix4f cursed$projection(Matrix4f projection) {
        cursed.recode.fx.AspectRatio.apply(projection);
        cursed.recode.engine.Upscale.jitter(projection);
        NativeTerrain.PROJECTION.set(projection);
        return projection;
    }
}
