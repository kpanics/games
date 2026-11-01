package cursed.recode.mixin.client;

import net.minecraft.client.renderer.MappableRingBuffer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Unique;
import org.spongepowered.asm.mixin.injection.Constant;
import org.spongepowered.asm.mixin.injection.ModifyConstant;

/**
 * The game's per-frame ring buffers (fog, transforms, clouds) hold four frames, not three: the renderer keeps four
 * submits in flight (vk/encoder.rs), and with three the game's thread waited for the GPU at the start of each frame
 * (FogRenderer.updateBuffer, 12.10: some 36 µs a frame) while the GPU was idle a sixth of the time. CURSED_DEV_RING=3:
 * the game's three.
 */
@Mixin(MappableRingBuffer.class)
public abstract class MappableRingBufferMixin {
    @Unique
    private static final int CURSED$COUNT = "3".equals(System.getenv("CURSED_DEV_RING")) ? 3 : 4;

    @ModifyConstant(method = {"<init>", "rotate", "close"}, constant = @Constant(intValue = 3))
    private int cursed$count(int three) {
        return CURSED$COUNT;
    }
}
