package cursed.recode.mixin.client;

import com.mojang.renderpearl.backend.vulkan.VulkanFeatureSets;
import cursed.recode.dev.Dev;
import net.minecraft.client.ClientBootstrap;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Redirect;

@Mixin(ClientBootstrap.class)
public abstract class ClientBootstrapMixin {
    /** The feature tables of Mojang's Vulkan backend are not built: that backend never runs. */
    @Redirect(method = "bootstrap", at = @At(value = "INVOKE", target = "Lcom/mojang/renderpearl/backend/vulkan/VulkanFeatureSets;bootstrap()V"))
    private static void cursed$noMojangVulkan() {
        if (Dev.mojang()) {
            VulkanFeatureSets.bootstrap();
        }
    }
}
