package cursed.recode.mixin.client;

import cursed.recode.dev.Dev;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import net.minecraft.client.renderer.rendertype.PreparedRenderType;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.Unique;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Redirect;

/**
 * A feature group's search for an earlier draw of an equal prepared render type (getOrAddDraw) by a hash map of first
 * occurrences instead of List.indexOf — the same index. Every switch of render type prepares it anew and writes a new
 * transform, so the draws pile up and indexOf compared each with records and lists, over and over (seen at ~10% of the
 * render thread with a few hundred dropped items). CURSED_DEV_NODRAWINDEX keeps indexOf.
 */
@Mixin(targets = "net.minecraft.client.renderer.feature.RenderTypeFeatureRenderer$Group")
public abstract class RenderTypeGroupMixin {
    @Unique
    private static final boolean OFF = Dev.on("CURSED_DEV_NODRAWINDEX");
    @Shadow
    @Final
    private List<PreparedRenderType> drawRenderTypes;
    /** Made on first use: a field initializer is not run for this inner class's constructor. */
    @Unique
    private Map<PreparedRenderType, Integer> cursed$first;

    @Redirect(method = "getOrAddDraw", at = @At(value = "INVOKE", target = "Ljava/util/List;indexOf(Ljava/lang/Object;)I"))
    private int cursed$indexOf(List<PreparedRenderType> list, Object type) {
        if (OFF || list != this.drawRenderTypes) {
            return list.indexOf(type);
        }
        Integer i = this.cursed$first == null ? null : this.cursed$first.get(type);
        int found = i == null ? -1 : i;
        if (CHECK) {
            int game = list.indexOf(type);
            checked++;
            if (game != found) {
                differ++;
                com.mojang.logging.LogUtils.getLogger().warn("[draw index] {} where indexOf says {}", found, game);
            }
            if (checked % 50000 == 0) {
                com.mojang.logging.LogUtils.getLogger().info("[draw index] checked {}, {} differ", checked, differ);
            }
        }
        return found;
    }

    /** CURSED_DEV_DRAWINDEXCHECK: every lookup against indexOf. */
    @Unique
    private static final boolean CHECK = Dev.on("CURSED_DEV_DRAWINDEXCHECK");
    @Unique
    private static long checked;
    @Unique
    private static long differ;

    @Redirect(method = "getOrAddDraw", at = @At(value = "INVOKE", target = "Ljava/util/List;add(Ljava/lang/Object;)Z"))
    private boolean cursed$add(List<Object> list, Object value) {
        if (list == (Object) this.drawRenderTypes) {
            if (this.cursed$first == null) {
                this.cursed$first = new HashMap<>();
            }
            this.cursed$first.putIfAbsent((PreparedRenderType) value, list.size());
        }
        return list.add(value);
    }
}
