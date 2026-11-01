package cursed.recode.render;

import com.mojang.renderpearl.api.textures.AddressMode;
import com.mojang.renderpearl.api.textures.FilterMode;
import com.mojang.renderpearl.api.textures.GpuSampler;
import java.util.OptionalDouble;

final class CursedSampler implements GpuSampler {
    private final CursedDevice device;
    final long handle;
    private final AddressMode addressModeU;
    private final AddressMode addressModeV;
    private final FilterMode minFilter;
    private final FilterMode magFilter;
    private final int maxAnisotropy;
    private final OptionalDouble maxLod;
    private boolean closed;

    CursedSampler(CursedDevice device, AddressMode u, AddressMode v, FilterMode min, FilterMode mag, int maxAnisotropy, OptionalDouble maxLod) {
        this.device = device;
        this.addressModeU = u;
        this.addressModeV = v;
        this.minFilter = min;
        this.magFilter = mag;
        this.maxAnisotropy = maxAnisotropy;
        this.maxLod = maxLod;
        this.handle = Native.samplerCreate(device.r, address(u), address(v), filter(min), filter(mag), maxAnisotropy,
                (float) maxLod.orElse(0.0), maxLod.isPresent());
    }

    private static int address(AddressMode mode) {
        return switch (mode) {
            case REPEAT -> 0;
            case CLAMP_TO_EDGE -> 1;
        };
    }

    private static int filter(FilterMode mode) {
        return switch (mode) {
            case NEAREST -> 0;
            case LINEAR -> 1;
        };
    }

    @Override
    public AddressMode getAddressModeU() {
        return this.addressModeU;
    }

    @Override
    public AddressMode getAddressModeV() {
        return this.addressModeV;
    }

    @Override
    public FilterMode getMinFilter() {
        return this.minFilter;
    }

    @Override
    public FilterMode getMagFilter() {
        return this.magFilter;
    }

    @Override
    public int getMaxAnisotropy() {
        return this.maxAnisotropy;
    }

    @Override
    public OptionalDouble getMaxLod() {
        return this.maxLod;
    }

    @Override
    public boolean isClosed() {
        return this.closed;
    }

    @Override
    public void close() {
        if (!this.closed) {
            this.closed = true;
            this.device.encoder.close(3, this.handle);
        }
    }
}
