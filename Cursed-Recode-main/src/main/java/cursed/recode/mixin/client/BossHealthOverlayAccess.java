package cursed.recode.mixin.client;

import java.util.Map;
import java.util.UUID;
import net.minecraft.client.gui.components.BossHealthOverlay;
import net.minecraft.client.gui.components.LerpingBossEvent;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

/** The boss bars shown: the HUD tells when a server's PvP mode (its boss bar) ends. */
@Mixin(BossHealthOverlay.class)
public interface BossHealthOverlayAccess {
    @Accessor("events")
    Map<UUID, LerpingBossEvent> cursed$events();
}
