package cursed.recode.mixin.client;

import com.mojang.blaze3d.systems.RenderSystem;
import com.mojang.renderpearl.api.pipeline.ShaderSource;
import com.mojang.renderpearl.api.pipeline.ShaderType;
import com.mojang.renderpearl.backend.api.SpvModule;
import com.mojang.renderpearl.frontend.shaders.GlslCompiler;
import cursed.recode.render.ShaderCache;
import net.minecraft.client.renderer.ShaderDefines;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Compiled shaders are kept on disk (cursed.recode.render.ShaderCache). */
@Mixin(GlslCompiler.class)
public abstract class GlslCompilerMixin {
    @Shadow
    @Final
    private boolean isZeroToOne;
    @Shadow
    @Final
    private boolean shaderDrawParameters;

    @Inject(method = "compileToSpv", at = @At("HEAD"), cancellable = true)
    private void cursed$cached(String name, String source, ShaderType type, ShaderDefines shaderDefines, ShaderSource shaderSource,
                               CallbackInfoReturnable<SpvModule> cir) throws com.mojang.renderpearl.util.ShaderCompileException {
        if (type == ShaderType.VERTEX && cursed.recode.render.TerrainCompact.SHADER.equals(name)) {
            String patched = cursed.recode.render.TerrainCompact.patch(source);
            if (patched != null) {
                // The patched text goes through this same method (and so the cache) once more.
                SpvModule module = ((GlslCompiler) (Object) this).compileToSpv(name, patched, type, shaderDefines, shaderSource);
                cursed.recode.render.TerrainCompact.mark(module);
                cir.setReturnValue(module);
                return;
            }
            if (!source.contains("cursed_Position")) {
                cursed.recode.render.TerrainCompact.unpatched();
            }
        }
        // Improved Transparency without its depth bounds (cursed.recode.render.CheapOit)
        if (type == ShaderType.FRAGMENT) {
            String cheap = cursed.recode.render.CheapOit.patch(source);
            if (!cheap.equals(source)) {
                cir.setReturnValue(((GlslCompiler) (Object) this).compileToSpv(name, cheap, type, shaderDefines, shaderSource));
                return;
            }
        }
        if (type == ShaderType.FRAGMENT && cursed.recode.render.TerrainCompact.SHADER.equals(name)) {
            // Merged faces repeat their sprite (TerrainCompact.patchFragment); the patched text comes through here again.
            String patched = cursed.recode.render.TerrainCompact.patchFragment(source);
            if (!patched.equals(source)) {
                cir.setReturnValue(((GlslCompiler) (Object) this).compileToSpv(name, patched, type, shaderDefines, shaderSource));
                return;
            }
        }
        if (type == ShaderType.FRAGMENT && cursed.recode.render.TerrainCompact.SHADER.equals(name) && System.getenv("CURSED_DEV_FSEXP") != null && !source.contains("cursed_fsexp")) {
            String patched = cursed.recode.render.TerrainCompact.devFragment(source) + "\n// cursed_fsexp\n";
            cir.setReturnValue(((GlslCompiler) (Object) this).compileToSpv(name, patched, type, shaderDefines, shaderSource));
            return;
        }
        // The compiler's switches, as createBaseShaderOptions sets them.
        String switches = "zeroToOne=" + this.isZeroToOne + " drawParameters=" + this.shaderDrawParameters
                + " explicitDepth=" + RenderSystem.getDevice().getDeviceInfo().hintsAndWorkarounds().isExplicitDepthRequired();
        SpvModule cached = ShaderCache.load(name, source, type, shaderDefines, shaderSource, switches);
        if (cached != null) {
            this.cursed$twin(name, source, type, shaderDefines, shaderSource, cached);
            cir.setReturnValue(cached);
        }
    }

    /** The engine's lit twin of a creature shader (cursed.recode.render.EntityEngine), compiled the same way. */
    @org.spongepowered.asm.mixin.Unique
    private void cursed$twin(String name, String source, ShaderType type, ShaderDefines shaderDefines, ShaderSource shaderSource, SpvModule game) {
        if (!cursed.recode.render.EntityEngine.wants(name, type, shaderDefines, source)) {
            return;
        }
        String patched = cursed.recode.render.EntityEngine.patch(name, source, type, shaderDefines);
        if (patched == null) {
            return;
        }
        try {
            SpvModule twin = ((GlslCompiler) (Object) this).compileToSpv(name, patched, type, shaderDefines, shaderSource);
            cursed.recode.render.EntityEngine.remember(game, twin);
        } catch (Exception e) {
            com.mojang.logging.LogUtils.getLogger().warn("[engine] the lit twin of {} ({}) failed, the game's light stays", name, type, e);
        }
    }

    @Inject(method = "compileToSpv", at = @At("RETURN"))
    private void cursed$store(String name, String source, ShaderType type, ShaderDefines shaderDefines, ShaderSource shaderSource,
                              CallbackInfoReturnable<SpvModule> cir) {
        ShaderCache.store(cir.getReturnValue());
        this.cursed$twin(name, source, type, shaderDefines, shaderSource, cir.getReturnValue());
    }
}
