package cursed.recode.render;

import com.mojang.renderpearl.api.GpuFormat;
import com.mojang.renderpearl.api.textures.GpuTexture;
import com.mojang.renderpearl.backend.common.BaseGpuTexture;

/** A texture of the game (vk/resources.rs Texture); it goes once it and all its views are closed. */
final class CursedTexture extends BaseGpuTexture {
    final CursedDevice device;
    final long handle;
    private boolean closed;

    CursedTexture(CursedDevice device, @GpuTexture.Usage int usage, String label, GpuFormat format, int width, int height, int depthOrLayers, int mipLevels) {
        super(usage, label, format, width, height, depthOrLayers, mipLevels);
        this.device = device;
        this.handle = Native.textureCreate(device.r, usage, VkMap.format(format), VkMap.aspect(format), width, height, depthOrLayers, mipLevels);
        device.encoder.initTexture(this.handle);
        device.name(1, this.handle, label);
    }

    static long handle(GpuTexture texture) {
        return ((CursedTexture) texture).handle;
    }

    @Override
    public boolean isClosed() {
        return this.closed;
    }

    @Override
    public void close() {
        if (!this.closed) {
            this.closed = true;
            this.device.encoder.close(1, this.handle);
        }
    }
}
