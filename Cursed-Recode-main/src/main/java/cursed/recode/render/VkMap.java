package cursed.recode.render;

import com.mojang.renderpearl.api.GpuFormat;
import com.mojang.renderpearl.api.pipeline.BlendFactor;
import com.mojang.renderpearl.api.pipeline.BlendOp;
import com.mojang.renderpearl.api.pipeline.CompareOp;
import com.mojang.renderpearl.api.pipeline.PolygonMode;
import com.mojang.renderpearl.api.pipeline.PrimitiveTopology;
import com.mojang.renderpearl.api.pipeline.ShaderType;

/**
 * The game's enums as Vulkan numbers, the same table as Mojang's VulkanConst. Switches over the enums on purpose: when a
 * new game version adds a constant, this stops compiling instead of sending a wrong number.
 */
final class VkMap {
    private VkMap() {
    }

    static int format(GpuFormat format) {
        return switch (format) {
            case R8_UNORM -> 9;
            case R8_SNORM -> 10;
            case RG8_UNORM -> 16;
            case RG8_SNORM -> 17;
            case RGB8_UNORM -> 23;
            case RGB8_SNORM -> 24;
            case RGBA8_UNORM -> 37;
            case RGBA8_SNORM -> 38;
            case R16_UNORM -> 70;
            case R16_SNORM -> 71;
            case RG16_UNORM -> 77;
            case RG16_SNORM -> 78;
            case RGB16_UNORM -> 84;
            case RGB16_SNORM -> 85;
            case RGBA16_UNORM -> 91;
            case RGBA16_SNORM -> 92;
            case R8_UINT -> 13;
            case R8_SINT -> 14;
            case RG8_UINT -> 20;
            case RG8_SINT -> 21;
            case RGB8_UINT -> 27;
            case RGB8_SINT -> 28;
            case RGBA8_UINT -> 41;
            case RGBA8_SINT -> 42;
            case R16_UINT -> 74;
            case R16_SINT -> 75;
            case RG16_UINT -> 81;
            case RG16_SINT -> 82;
            case RGB16_UINT -> 88;
            case RGB16_SINT -> 89;
            case RGBA16_UINT -> 95;
            case RGBA16_SINT -> 96;
            case R32_UINT -> 98;
            case R32_SINT -> 99;
            case RG32_UINT -> 101;
            case RG32_SINT -> 102;
            case RGB32_UINT -> 104;
            case RGB32_SINT -> 105;
            case RGBA32_UINT -> 107;
            case RGBA32_SINT -> 108;
            case R16_FLOAT -> 76;
            case RG16_FLOAT -> 83;
            case RGB16_FLOAT -> 90;
            case RGBA16_FLOAT -> 97;
            case R32_FLOAT -> 100;
            case RG32_FLOAT -> 103;
            case RGB32_FLOAT -> 106;
            case RGBA32_FLOAT -> 109;
            case RGB10A2_UNORM -> 64;
            case RGB10A2_UINT -> 68;
            case RG11B10_FLOAT -> 122;
            case D32_FLOAT -> 126;
            case D32_FLOAT_S8_UINT -> 130;
            case D24_UNORM_S8_UINT -> 129;
            case D16_UNORM -> 124;
            case S8_UINT -> 127;
        };
    }

    /** VkImageAspectFlags: colour 1, depth 2, stencil 4. */
    static int aspect(GpuFormat format) {
        int a = 0;
        if (format.hasColorAspect()) {
            a |= 1;
        }
        if (format.hasDepthAspect()) {
            a |= 2;
        }
        if (format.hasStencilAspect()) {
            a |= 4;
        }
        return a;
    }

    static int blendFactor(BlendFactor factor) {
        return switch (factor) {
            case CONSTANT_ALPHA -> 12;
            case CONSTANT_COLOR -> 10;
            case ONE_MINUS_CONSTANT_ALPHA -> 13;
            case ONE_MINUS_CONSTANT_COLOR -> 11;
            case DST_ALPHA -> 8;
            case DST_COLOR -> 4;
            case ONE -> 1;
            case ONE_MINUS_DST_ALPHA -> 9;
            case ONE_MINUS_DST_COLOR -> 5;
            case ONE_MINUS_SRC_ALPHA -> 7;
            case ONE_MINUS_SRC_COLOR -> 3;
            case SRC_ALPHA -> 6;
            case SRC_ALPHA_SATURATE -> 14;
            case SRC_COLOR -> 2;
            case ZERO -> 0;
        };
    }

    static int blendOp(BlendOp op) {
        return switch (op) {
            case ADD -> 0;
            case SUBTRACT -> 1;
            case REVERSE_SUBTRACT -> 2;
            case MIN -> 3;
            case MAX -> 4;
        };
    }

    static int compareOp(CompareOp op) {
        return switch (op) {
            case ALWAYS_PASS -> 7;
            case LESS_THAN -> 1;
            case LESS_THAN_OR_EQUAL -> 3;
            case EQUAL -> 2;
            case NOT_EQUAL -> 5;
            case GREATER_THAN_OR_EQUAL -> 6;
            case GREATER_THAN -> 4;
            case NEVER_PASS -> 0;
        };
    }

    static int polygonMode(PolygonMode mode) {
        return switch (mode) {
            case FILL -> 0;
            case WIREFRAME -> 1;
        };
    }

    /** Lines and quads are triangles by the time they reach the GPU (the game expands them). */
    static int topology(PrimitiveTopology topology) {
        return switch (topology) {
            case LINES -> 3;
            case DEBUG_LINES -> 1;
            case DEBUG_LINE_STRIP -> 2;
            case POINTS -> 0;
            case TRIANGLES -> 3;
            case TRIANGLE_STRIP -> 4;
            case TRIANGLE_FAN -> 5;
            case QUADS -> 3;
        };
    }

    static int stage(ShaderType type) {
        return switch (type) {
            case VERTEX -> 1;
            case FRAGMENT -> 16;
        };
    }
}
