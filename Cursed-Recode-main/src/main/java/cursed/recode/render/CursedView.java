package cursed.recode.render;

import com.mojang.renderpearl.api.textures.GpuTextureView;
import com.mojang.renderpearl.backend.common.BaseGpuTextureView;

/** A view of some mip levels of a texture (vk/resources.rs View). */
final class CursedView extends BaseGpuTextureView {
    private final CursedDevice device;
    final long handle;
    private boolean closed;

    CursedView(CursedDevice device, CursedTexture texture, int baseMipLevel, int mipLevels) {
        super(texture, baseMipLevel, mipLevels);
        this.device = device;
        this.handle = Native.viewCreate(device.r, texture.handle, baseMipLevel, mipLevels);
    }

    static long handle(GpuTextureView view) {
        return ((CursedView) view).handle;
    }

    @Override
    public boolean isClosed() {
        return this.closed;
    }

    @Override
    public void close() {
        if (!this.closed) {
            this.closed = true;
            this.device.encoder.close(2, this.handle);
        }
    }
}
