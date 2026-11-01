package cursed.recode.mixin.client;

import com.mojang.blaze3d.platform.NativeLibrariesBootstrap;
import cursed.recode.dev.Dev;
import java.util.List;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Redirect;

/**
 * The game's native libraries without Mojang's renderers: no OpenGL driver, no LWJGL Vulkan instance, no VMA (their
 * memory allocator). The shader compilers (shaderc, spvc) stay: the game's frontend compiles its shaders with them.
 */
@Mixin(NativeLibrariesBootstrap.class)
public abstract class NativeLibrariesBootstrapMixin {
    /** "Vulkan is there" without LWJGL loading it: our DLL loads the Vulkan loader itself, and the shader compilers get loaded. */
    @Redirect(method = "loadLibraries", at = @At(value = "INVOKE", target = "Lcom/mojang/blaze3d/platform/NativeLibrariesBootstrap;tryLoadingVulkan()Z"))
    private static boolean cursed$noLwjglVulkan() {
        if (Dev.mojang()) {
            try {
                // As the game does: without this LWJGL creates Vulkan by itself and a second create fails.
                if (org.lwjgl.system.Configuration.VULKAN_EXPLICIT_INIT.get() == null) {
                    org.lwjgl.system.Configuration.VULKAN_EXPLICIT_INIT.set(true);
                }
                org.lwjgl.vulkan.VK.create();
                return true;
            } catch (Throwable t) {
                return false;
            }
        }
        return true;
    }

    @Redirect(method = "loadLibraries", at = @At(value = "INVOKE", target = "Ljava/util/List;add(Ljava/lang/Object;)Z"))
    private static boolean cursed$skipMojangRenderers(List<Object> list, Object entry) {
        String name = ((NativeLibrariesBootstrap.LibraryLoadEntry) entry).name();
        if (!Dev.mojang() && (name.equals("OpenGL") || name.equals("vma"))) {
            return false;
        }
        return list.add(entry);
    }
}
