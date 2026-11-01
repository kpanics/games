package cursed.recode.mixin.client;

import net.minecraft.client.renderer.StagedVertexBuffer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

/** A draw's places in the frame's vertex and index buffers (cursed.recode.render.DirectStaging's upload). */
@Mixin(StagedVertexBuffer.Draw.class)
public interface StagedDrawAccessor {
    @Accessor("vertexOffset")
    int cursed$vertexOffset();

    @Accessor("indexOffset")
    int cursed$indexOffset();
}
