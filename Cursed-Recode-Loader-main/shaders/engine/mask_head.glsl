#version 450
// The shadow mask: for every pixel of the opaque terrain (its depth laid down by the depth pass), how much of the sun
// (or the moon) reaches it, traced once through the voxel window. The lit terrain pass reads it (terrain.fsh,
// CURSED_SHADE). vk/shaders.rs puts this, trace.glsl and mask.comp together.

layout(local_size_x = 8, local_size_y = 8) in;

layout(set = 0, binding = 0) uniform sampler2D Depth;
layout(set = 0, binding = 1) uniform usampler2DArray CursedVoxels;
layout(set = 0, binding = 2) uniform usampler2DArray CursedBricks;
// Half the resolution: the light let through, and the depth of the pixel it was traced for.
layout(set = 0, binding = 3, rg32f) uniform writeonly image2D Mask;
layout(std140, set = 0, binding = 4) uniform CursedEngine {
    ivec4 CursedWindow;
    ivec4 CursedSize;
    vec4 CursedSun;
    vec4 CursedSunColor;
    vec4 CursedMoon;
    vec4 CursedSky;
    vec4 CursedTime;
    vec4 CursedSpare;
    ivec4 CursedColumns[64];
    ivec4 CursedCameraBlock;
    vec4 CursedCameraFrac;
    vec4 CursedEntities[64];
};
#define CURSED_ENTITY_SHADOWS

layout(push_constant) uniform Frame {
    // Clip space back to round the camera (the terrain's ProjMat * ModelViewMat, inverted).
    mat4 InvViewProj;
    // The camera's block, and the camera from it.
    ivec4 Base;
    vec4 Frac;
    // Width, height; the mask's width, height.
    ivec4 Size;
};

// trace.glsl reads it as the terrain shader's uniform of the same name.
ivec3 CameraBlockPos;

