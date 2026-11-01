//! This renderer's own shaders (compute, and the impostor's vertex and fragment shaders), as GLSL. Java compiles them with the game's shaderc and hands the SPIR-V back
//! (lib.rs shaders_init), so the DLL carries no compiler of its own. The order is the indices Java sends back.

pub const SOURCES: &[(&str, &str)] = &[
    ("models", include_str!("../../shaders/models.comp")),
    ("occlusion_prep", include_str!("../../shaders/occlusion_prep.comp")),
    ("occlusion_reduce", include_str!("../../shaders/occlusion_reduce.comp")),
    ("occlusion_cull", include_str!("../../shaders/occlusion_cull.comp")),
    (
        "engine_mask",
        concat!(
            include_str!("../../shaders/engine/mask_head.glsl"),
            include_str!("../../shaders/engine/trace.glsl"),
            include_str!("../../shaders/engine/mask.comp")
        ),
    ),
    ("engine_taa", include_str!("../../shaders/engine/taa.comp")),
    ("engine_light", include_str!("../../shaders/engine/light.comp")),
    ("engine_water", include_str!("../../shaders/engine/water.comp")),
    (
        "engine_sunvis",
        concat!(
            include_str!("../../shaders/engine/sunvis_head.glsl"),
            include_str!("../../shaders/engine/trace.glsl"),
            include_str!("../../shaders/engine/sunvis.comp")
        ),
    ),
    ("engine_volume", include_str!("../../shaders/engine/volume.comp")),
    ("engine_glow", include_str!("../../shaders/engine/glow.comp")),
    (
        "engine_skylut",
        concat!(
            include_str!("../../shaders/engine/sky_lut_head.glsl"),
            include_str!("../../shaders/engine/sky.glsl"),
            include_str!("../../shaders/engine/sky_lut.comp")
        ),
    ),
    ("impostor.vert", include_str!("../../shaders/impostor.vert")),
    ("impostor.frag", include_str!("../../shaders/impostor.frag")),
    ("ink_sim", include_str!("../../shaders/engine/ink_sim.comp")),
    ("ink_draw", include_str!("../../shaders/engine/ink_draw.comp")),
    ("ink_comp", include_str!("../../shaders/engine/ink_comp.comp")),
    ("ink_part", include_str!("../../shaders/engine/ink_part.comp")),
    ("chams_ground", include_str!("../../shaders/engine/ground.comp")),
    ("chams_liquid", include_str!("../../shaders/engine/liquid.comp")),
    ("chams_curse", include_str!("../../shaders/engine/curse.comp")),
    ("target_esp", include_str!("../../shaders/engine/tesseract.comp")),
    ("chams_shadow", include_str!("../../shaders/engine/shadow.comp")),
    ("hitboxes", include_str!("../../shaders/engine/hitboxes.comp")),
    ("fx", include_str!("../../shaders/engine/fx.comp")),
    ("viewmodel.vert", include_str!("../../shaders/cosmetics/viewmodel.vert")),
    ("viewmodel.frag", concat!(include_str!("../../shaders/cosmetics/light.glsl"), include_str!("../../shaders/cosmetics/viewmodel.frag"))),
    ("scope.vert", include_str!("../../shaders/cosmetics/scope.vert")),
    ("scope.frag", include_str!("../../shaders/cosmetics/scope.frag")),
    ("finish.vert", include_str!("../../shaders/cosmetics/finish.vert")),
    ("glove_finish.vert", include_str!("../../shaders/cosmetics/glove_finish.vert")),
    ("sticker.vert", include_str!("../../shaders/cosmetics/sticker.vert")),
    ("sticker.frag", concat!(include_str!("../../shaders/cosmetics/light.glsl"), include_str!("../../shaders/cosmetics/sticker.frag"))),
    ("resolve.frag", include_str!("../../shaders/cosmetics/resolve.frag")),
];

pub const MODELS: usize = 0;
pub const OCCLUSION: [usize; 3] = [1, 2, 3];
pub const ENGINE_MASK: usize = 4;
pub const ENGINE_TAA: usize = 5;
pub const ENGINE_LIGHT: usize = 6;
pub const ENGINE_WATER: usize = 7;
pub const ENGINE_SUNVIS: usize = 8;
pub const ENGINE_VOLUME: usize = 9;
pub const ENGINE_GLOW: usize = 10;
pub const ENGINE_SKYLUT: usize = 11;
/// The far terrain's faces laid over the frame (vk/impostor.rs): vertex and fragment shader.
pub const IMPOSTOR: [usize; 2] = [12, 13];
/// The chams' ink (engine/ink.rs): its simulation, the ray through it, it over the frame.
pub const INK: [usize; 4] = [14, 15, 16, 17];
/// The chams' traces on the ground (engine/ground.rs).
pub const GROUND: usize = 18;
/// The chams' polymorphic liquid (engine/liquid.rs).
pub const LIQUID: usize = 19;
/// The chams' curse in the air: its ash (engine/curse.rs).
pub const CURSE: usize = 20;
/// Target ESP's tesseract (engine/tesseract.rs).
pub const TESSERACT: usize = 21;
/// The chams' shadow in the air: its smoke (engine/shadow.rs).
pub const SHADOW: usize = 22;
/// Hitboxes (engine/hitboxes.rs).
pub const HITBOXES: usize = 23;
/// The world's effects (engine/fx.rs).
pub const FX: usize = 24;
/// The cosmetics' models in the hands (cosmetics/render.rs): vertex and fragment shader.
pub const VIEWMODEL: [usize; 2] = [25, 26];
/// A sniper's scope over the picture (cosmetics/render.rs): vertex and fragment shader.
pub const SCOPE: [usize; 2] = [27, 28];
/// A finish worked in a weapon's texture space (cosmetics/render.rs; its fragment shader is CS2's own).
pub const FINISH_VS: usize = 29;
/// A glove paint worked in the gloves' texture space (its fragment shader is CS2's own).
pub const GLOVE_FINISH_VS: usize = 30;
/// A sticker over a body (cosmetics/render.rs; its surface CS2's sticker shader's): vertex and fragment shader.
pub const STICKER: [usize; 2] = [31, 32];
/// The page's look drawn larger brought down into the menu's picture (cosmetics/render.rs frame_large).
pub const RESOLVE: usize = 33;

/// shaders_init's flags.
pub const CHECK_MODELS: i32 = 1;
pub const NO_OCCLUSION: i32 = 2;
