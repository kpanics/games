package cursed.recode.mixin.client;

import com.mojang.renderpearl.api.pipeline.IndexType;
import cursed.recode.render.NativeTerrain;
import cursed.recode.render.SectionMirror;
import cursed.recode.render.TerrainMeshHolder;
import java.nio.ByteBuffer;
import java.util.concurrent.atomic.AtomicReference;
import net.minecraft.core.BlockPos;
import net.minecraft.client.renderer.chunk.ChunkSectionLayer;
import net.minecraft.client.renderer.chunk.CompiledSectionMesh;
import net.minecraft.client.renderer.chunk.SectionMesh;
import org.jspecify.annotations.Nullable;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.Unique;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/**
 * A compiled section's geometry goes to the native terrain instead of the game's buffers; what the frame reads of the
 * section is kept on it (cursed.recode.render.SectionMirror).
 */
@Mixin(targets = "net.minecraft.client.renderer.chunk.SectionRenderDispatcher$RenderSection")
public abstract class RenderSectionMixin implements SectionMirror {
    @Shadow
    @Final
    public AtomicReference<SectionMesh> sectionMesh;

    @Shadow
    @Final
    private BlockPos.MutableBlockPos renderOrigin;

    @Unique
    private volatile SectionMesh cursed$plain = CompiledSectionMesh.UNCOMPILED;
    @Unique
    private volatile boolean cursed$blockEntities;
    @Unique
    private volatile int cursed$drawId;
    @Unique
    private volatile boolean cursed$renderable;
    @Unique
    private int cursed$x;
    @Unique
    private int cursed$y;
    @Unique
    private int cursed$z;
    @Unique
    private Object cursed$state;
    @Unique
    private Object cursed$stateOwner;

    @Inject(method = "setSectionMesh", at = @At("RETURN"))
    private void cursed$meshSet(SectionMesh mesh, CallbackInfoReturnable<SectionMesh> cir) {
        this.cursed$mirror();
    }

    @Inject(method = "reset", at = @At("TAIL"))
    private void cursed$meshReset(CallbackInfo ci) {
        this.cursed$mirror();
    }

    @Inject(method = "setSectionNode", at = @At("TAIL"))
    private void cursed$origin(long sectionNode, CallbackInfo ci) {
        this.cursed$x = this.renderOrigin.getX();
        this.cursed$y = this.renderOrigin.getY();
        this.cursed$z = this.renderOrigin.getZ();
    }

    @Unique
    private void cursed$mirror() {
        SectionMesh mesh = this.sectionMesh.get();
        this.cursed$blockEntities = !mesh.getRenderableBlockEntities().isEmpty();
        // under the section's lock, as the mesh's id changes reach it (CompiledSectionMeshMixin.cursed$setMesh): owner
        // first, then the id, so an id given meanwhile is either read here or sent here
        synchronized (this) {
            this.cursed$renderable = mesh instanceof CompiledSectionMesh && mesh.hasRenderableLayers();
            this.cursed$plain = mesh;
            if (mesh instanceof TerrainMeshHolder holder) {
                holder.cursed$setOwner(this);
                this.cursed$drawId = holder.cursed$mesh();
            } else {
                this.cursed$drawId = 0;
            }
        }
    }

    @Override
    public int cursed$drawId() {
        return this.cursed$renderable ? this.cursed$drawId : 0;
    }

    @Override
    public void cursed$setDrawId(int id) {
        this.cursed$drawId = id;
    }

    @Override
    public SectionMesh cursed$plainMesh() {
        return this.cursed$plain;
    }

    @Override
    public boolean cursed$hasBlockEntities() {
        return this.cursed$blockEntities;
    }

    @Override
    public int cursed$originX() {
        return this.cursed$x;
    }

    @Override
    public int cursed$originY() {
        return this.cursed$y;
    }

    @Override
    public int cursed$originZ() {
        return this.cursed$z;
    }

    @Override
    public Object cursed$state() {
        return this.cursed$state;
    }

    @Override
    public Object cursed$stateOwner() {
        return this.cursed$stateOwner;
    }

    @Override
    public void cursed$setState(Object state, Object owner) {
        this.cursed$state = state;
        this.cursed$stateOwner = owner;
    }

    @Shadow
    private void vertexBufferUploadCallback(final CompiledSectionMesh sectionMesh, final ChunkSectionLayer layer) {
    }

    @Shadow
    private void indexBufferUploadCallback(final CompiledSectionMesh sectionMesh, final ChunkSectionLayer layer, final boolean sortedIndexBuffer) {
    }

    @Inject(method = "addSectionBuffersToUberBuffer", at = @At("HEAD"), cancellable = true)
    private void cursed$toNative(ChunkSectionLayer layer, CompiledSectionMesh key, @Nullable ByteBuffer vertexBuffer, @Nullable ByteBuffer indexBuffer,
                                 CallbackInfoReturnable<Boolean> cir) {
        if (!NativeTerrain.active()) {
            return;
        }
        SectionMesh.SectionDraw draw = key.getSectionDraw(layer);
        if (draw != null) {
            TerrainMeshHolder holder = (TerrainMeshHolder) key;
            int id = NativeTerrain.meshOf(holder);
            NativeTerrain.upload(id, layer.ordinal(), vertexBuffer, indexBuffer, draw.indexCount(), draw.indexType() == IndexType.INT, holder.cursed$facings(),
                    this.cursed$x, this.cursed$y, this.cursed$z);
            boolean hasVertices = vertexBuffer != null;
            boolean hasIndices = indexBuffer != null;
            if (!hasIndices) {
                key.setIndexBufferUploaded(layer);
            }
            // As the game's upload callbacks: on the render thread, once the data is on its way to the GPU.
            NativeTerrain.afterUpload(() -> {
                if (holder.cursed$mesh() == 0) {
                    return;
                }
                if (hasVertices) {
                    this.vertexBufferUploadCallback(key, layer);
                }
                if (hasIndices) {
                    this.indexBufferUploadCallback(key, layer, !hasVertices);
                }
            });
        }
        cir.setReturnValue(true);
    }

    @Inject(method = "releaseSectionMesh", at = @At("HEAD"))
    private void cursed$release(SectionMesh oldMesh, CallbackInfo ci) {
        if (oldMesh instanceof TerrainMeshHolder holder) {
            NativeTerrain.release(holder);
        }
    }
}
