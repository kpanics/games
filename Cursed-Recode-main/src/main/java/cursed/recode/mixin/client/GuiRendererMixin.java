package cursed.recode.mixin.client;

import com.mojang.blaze3d.ProjectionType;
import com.mojang.blaze3d.pipeline.RenderTarget;
import com.mojang.blaze3d.systems.RenderSystem;
import com.mojang.renderpearl.api.buffers.GpuBufferSlice;
import cursed.recode.dev.FeatureTimes;
import cursed.recode.scene.HudCache;
import java.util.List;
import java.util.function.Supplier;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.render.GuiRenderer;
import net.minecraft.client.renderer.Projection;
import net.minecraft.client.renderer.ProjectionMatrixBuffer;
import net.minecraft.client.renderer.state.WindowRenderState;
import org.joml.Matrix4f;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * The HUD drawn into its texture as well on the frames that draw it (cursed.recode.scene.HudCache); dev timing of the
 * GUI's preparation and drawing (CURSED_DEV_BENCH).
 */
@Mixin(GuiRenderer.class)
public abstract class GuiRendererMixin {
    @Shadow @Final private net.minecraft.client.renderer.state.gui.GuiRenderState renderState;
    @Shadow @Final private List<Object> draws;
    @Shadow private int firstDrawIndexAfterBlur;
    @Shadow @Final private Projection guiProjection;
    @Shadow @Final private ProjectionMatrixBuffer guiProjectionMatrixBuffer;
    @Shadow @Final private net.minecraft.client.renderer.feature.FeatureRenderDispatcher featureRenderDispatcher;

    @Shadow
    private void executeDrawRange(Supplier<String> label, RenderTarget mainRenderTarget, GpuBufferSlice dynamicTransforms, int startIndex, int endIndex) {
    }

    /** CURSED_DEV_STALLS: a frame that came late goes to the log, with the screen up (what held it up is near it). */
    private static final boolean STALLS = cursed.recode.dev.Dev.on("CURSED_DEV_STALLS");
    private static long lastFrame;

    @Inject(method = "render", at = @At("HEAD"))
    private void cursed$start(CallbackInfo ci) {
        // the menus' own pace, or the game's: the swap chain made again only when that changes
        cursed.recode.launch.LaunchScreen.checkPace();
        cursed.recode.launch.LaunchScreen.quitFrame();
        cursed.recode.launch.Transition.frame();
        cursed.recode.launch.Transition.beforeGui();
        cursed.recode.menus.MenuWorld.frame();
        if (STALLS) {
            long t = System.nanoTime();
            if (lastFrame != 0 && t - lastFrame > 40_000_000L) {
                net.minecraft.client.gui.screens.Screen s = Minecraft.getInstance().gui.screen();
                cursed.recode.CursedRecode.LOGGER.info("[stall] {} ms before this frame, screen {}", (t - lastFrame) / 1_000_000, s == null ? "-" : s.getClass().getSimpleName());
            }
            lastFrame = t;
        }
        FeatureTimes.start("gui");
        HudCache.cover((HudCache.GuiStrata) this.renderState);
    }

    @Inject(method = "render", at = @At("TAIL"))
    private void cursed$stop(CallbackInfo ci) {
        FeatureTimes.stop("gui");
    }

    /**
     * A frame that draws the HUD into its texture (cursed.recode.scene.HudCache): everything as the game draws it with
     * no blur (one pass, the same projection and transform), then the strata after the split (the HUD above the
     * crosshair, there instead of a screen's blur) once more into the texture.
     */
    @Inject(method = "draw", at = @At("HEAD"), cancellable = true)
    private void cursed$hudCache(CallbackInfo ci) {
        if (!HudCache.drawing()) {
            return;
        }
        ci.cancel();
        Minecraft minecraft = Minecraft.getInstance();
        WindowRenderState window = minecraft.gameRenderer.gameRenderState().windowRenderState;
        this.guiProjection.setupOrtho(1000.0F, 11000.0F, (float) window.width / window.guiScale, (float) window.height / window.guiScale, true);
        RenderSystem.setProjectionMatrix(this.guiProjectionMatrixBuffer.getBuffer(this.guiProjection), ProjectionType.ORTHOGRAPHIC);
        RenderTarget main = minecraft.gameRenderer.mainRenderTarget();
        GpuBufferSlice transforms = RenderSystem.getDynamicUniforms().writeTransform(new Matrix4f().setTranslation(0.0F, 0.0F, -11000.0F));
        int size = this.draws.size();
        int split = Math.min(this.firstDrawIndexAfterBlur, size);
        if (size > 0) {
            this.executeDrawRange(() -> "GUI before blur", main, transforms, 0, size);
        }
        RenderTarget cache = HudCache.target(main);
        if (size > split) {
            this.executeDrawRange(() -> "Cursed HUD", cache, transforms, split, size);
        }
        HudCache.drawn();
    }

    /** The menus' items of the frame (cursed.recode.menus.ItemStage): drawn into its texture, a blit each. */
    @Inject(method = "preparePictureInPictureState", at = @At("HEAD"), cancellable = true)
    private void cursed$itemStage(net.minecraft.client.renderer.state.gui.pip.PictureInPictureRenderState state, int guiScale, CallbackInfo ci) {
        if (state instanceof cursed.recode.menus.ItemStage.Frame frame) {
            ci.cancel();
            cursed.recode.menus.ItemStage.prepare(frame, this.renderState, this.featureRenderDispatcher);
        }
    }

    /** A frame that keeps the HUD's texture uses none of the oversized items' renderers: they stay for the next one that draws it. */
    @Inject(method = "clearUnusedOversizedItemRenderers", at = @At("HEAD"), cancellable = true)
    private void cursed$keepOversized(CallbackInfo ci) {
        if (HudCache.kept()) {
            ci.cancel();
        }
    }
}
