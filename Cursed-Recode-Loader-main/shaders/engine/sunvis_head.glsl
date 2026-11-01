#version 450
// Declarations for sunvis.comp (the sun's reach per block); trace.glsl comes after them.

layout(local_size_x = 4, local_size_y = 4, local_size_z = 4) in;

layout(set = 0, binding = 0) uniform usampler2DArray CursedVoxels;
layout(set = 0, binding = 1) uniform usampler2DArray CursedBricks;
layout(set = 0, binding = 2, r8) uniform writeonly image3D SunVis;
layout(std140, set = 0, binding = 3) uniform CursedEngine {
    ivec4 CursedWindow;
    ivec4 CursedSize;
    vec4 CursedSun;
    vec4 CursedSunColor;
    vec4 CursedMoon;
    vec4 CursedSky;
    vec4 CursedTime;
    vec4 CursedSpare;
    ivec4 CursedColumns[64];
};
// Rows 2..257: the game's cloud map (trace.glsl, the clouds' shadow).
layout(set = 0, binding = 4) uniform sampler2D CursedPalette;
#define CURSED_CLOUD_SHADOWS

layout(push_constant) uniform Grid {
    // The grid's lowest corner in the world.
    ivec4 Origin;
    // The camera's block.
    ivec4 Base;
    // x: the frame's number.
    ivec4 Frame;
};

const ivec3 MASK = ivec3(127, 63, 127);

// trace.glsl reads it as the terrain shader's uniform of the same name.
ivec3 CameraBlockPos;

