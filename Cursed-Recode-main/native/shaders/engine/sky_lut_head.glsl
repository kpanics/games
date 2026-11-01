#version 450
// Declarations for sky_lut.comp (the sky's table); sky.glsl comes after them.

layout(local_size_x = 8, local_size_y = 8) in;

// The engine's palette: the table goes in its rows 258..321.
layout(set = 0, binding = 0, rgba32f) uniform writeonly image2D Palette;
layout(std140, set = 0, binding = 1) uniform CursedEngine {
    ivec4 CursedWindow;
    ivec4 CursedSize;
    vec4 CursedSun;
    vec4 CursedSunColor;
    vec4 CursedMoon;
    vec4 CursedSky;
    vec4 CursedTime;
    vec4 CursedSpare;
};
