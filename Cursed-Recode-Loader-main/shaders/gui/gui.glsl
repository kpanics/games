// Shared head of the menu's shaders (gui/render.rs): the frame's numbers. Pictures are stored row 0 at the top and
// everything is in the picture's px, y down, as gui/mod.rs lays it out.

layout(std140, set = 0, binding = 0) uniform Gui {
    vec4 Win;     // the window: x, y, width, height
    vec4 Open;    // unfolded (0..1), the line's length, its heat, its mode (0 opening, 1 closing)
    vec4 Look;    // veil (how far it has come), the sections' rail width, the corner radius, the whole's fade
    vec4 Cur;     // the cursor x, y, its light, time
    vec4 Size;    // picture width, height, the backdrop (0 none), where the shape indices start in the lists
    vec4 Field;   // atlas width, height, spread (atlas px), tiles across
    vec4 Glass;   // the theme's glass: its tint, how much of it covers the world
    vec4 Veil;    // the colour the world is dimmed with, how much
    vec4 Theme;   // how much of the world's colour comes through, the rail's shade, the sheen, the shadow
    vec4 Fire;    // the burning line's colour, the cursor's light (0 off)
    vec4 Tiles;   // the tiles drawn, if not all: where their list starts in the lists, how many (0: all)
    vec4 Anim;    // the glass opening's: the window's alpha, how frosted (0 the world sharp), a light across it (<0 none);
                  // how much of the world's colour drains away round the window (0 none)
};
