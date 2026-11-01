package cursed.recode.mixin.client;

import com.mojang.blaze3d.vertex.ByteBufferBuilder;
import cursed.recode.render.DirectStaging;
import org.jspecify.annotations.Nullable;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.Unique;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** A builder that writes into the GPU's memory (cursed.recode.render.DirectStaging) grows into a bigger slot and gives its own memory back to be freed. */
@Mixin(ByteBufferBuilder.class)
public abstract class ByteBufferBuilderMixin implements DirectStaging.Builder {
    @Shadow
    private long pointer;
    @Shadow
    private long capacity;
    @Shadow
    private long writeOffset;
    @Unique
    private @Nullable DirectStaging cursed$direct;

    @Inject(method = "resize", at = @At("HEAD"), cancellable = true)
    private void cursed$grow(long newCapacity, CallbackInfo ci) {
        if (this.cursed$direct != null && this.cursed$direct.on()) {
            this.cursed$direct.grow(newCapacity);
            ci.cancel();
        }
    }

    @Inject(method = "close", at = @At("HEAD"))
    private void cursed$close(CallbackInfo ci) {
        if (this.cursed$direct != null) {
            this.cursed$direct.close();
        }
    }

    @Override
    public long cursed$pointer() {
        return this.pointer;
    }

    @Override
    public long cursed$capacity() {
        return this.capacity;
    }

    @Override
    public long cursed$writeOffset() {
        return this.writeOffset;
    }

    @Override
    public void cursed$memory(long pointer, long capacity) {
        this.pointer = pointer;
        this.capacity = capacity;
    }

    @Override
    public @Nullable DirectStaging cursed$direct() {
        return this.cursed$direct;
    }

    @Override
    public void cursed$direct(@Nullable DirectStaging direct) {
        this.cursed$direct = direct;
    }
}
