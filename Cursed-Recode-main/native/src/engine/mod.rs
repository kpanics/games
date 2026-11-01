//! The engine: what Cursed Recode draws beyond the game's picture — the world as voxels on the GPU (voxels.rs,
//! gpu.rs) and what rays through it give: sun shadows on terrain (shaders/engine/terrain.fsh).

pub mod gpu;
pub mod ground;
pub mod ink;
pub mod liquid;
pub mod curse;
pub mod shadow;
pub mod tesseract;
pub mod hitboxes;
pub mod fx;
pub mod upscale;
pub mod voxels;

/// The engine's shaders for terrain, as GLSL bodies; Java puts the game's uniform declarations in front of them
/// (cursed.recode.engine.EngineShaders) and compiles them with the game's shaderc.
pub const TRACE: &str = include_str!("../../shaders/engine/trace.glsl");
pub const ENTITY_LIGHT: &str = include_str!("../../shaders/engine/entity_light.glsl");
pub const SKY: &str = include_str!("../../shaders/engine/sky.glsl");
pub const CHAMS: &str = include_str!("../../shaders/engine/chams.glsl");
pub const TERRAIN_VERTEX: &str = include_str!("../../shaders/engine/terrain.vsh");
pub const TERRAIN_FRAGMENT: &str = concat!(
    include_str!("../../shaders/engine/trace.glsl"),
    include_str!("../../shaders/engine/reflect.glsl"),
    include_str!("../../shaders/engine/terrain.fsh")
);
