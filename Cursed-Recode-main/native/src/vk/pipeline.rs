//! Render pipelines: the game's shaders (already SPIR-V) with the fixed state they draw with.
//!
//! The game compiles them on its worker threads; this file touches only the device, never the frame, so any thread
//! may call it. Java packs the description into bytes (CursedPipeline.pack); the layout is read below in the same
//! order. Enums arrive already as Vulkan numbers.
//!
//! As in Mojang's backend each pipeline has a variant for render passes with a depth attachment (D32_SFLOAT, the only
//! depth format the game renders to) and, when it does not test depth, one for passes without.

use super::device::Gpu;
use ash::vk;

#[derive(Clone, Copy)]
pub struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Reader { data, at: 0 }
    }

    fn take<const N: usize>(&mut self) -> [u8; N] {
        let bytes: [u8; N] = self.data[self.at..self.at + N].try_into().unwrap();
        self.at += N;
        bytes
    }

    pub fn i32(&mut self) -> i32 {
        i32::from_le_bytes(self.take())
    }

    pub fn u32(&mut self) -> u32 {
        self.i32() as u32
    }

    pub fn i64(&mut self) -> i64 {
        i64::from_le_bytes(self.take())
    }

    pub fn f32(&mut self) -> f32 {
        f32::from_le_bytes(self.take())
    }

    pub fn f64(&mut self) -> f64 {
        f64::from_le_bytes(self.take())
    }

    /// A string: its length, its UTF-8 bytes, then zeros up to a multiple of 4 so what follows stays aligned.
    pub fn str(&mut self) -> String {
        let n = self.i32().max(0) as usize;
        let s = String::from_utf8_lossy(&self.data[self.at..self.at + n]).into_owned();
        self.at += n.div_ceil(4) * 4;
        s
    }

    pub fn bytes(&mut self, n: usize) -> &'a [u8] {
        let b = &self.data[self.at..self.at + n];
        self.at += n;
        b
    }

    pub fn done(&self) -> bool {
        self.at >= self.data.len()
    }

    /// Where it is in its bytes.
    pub fn at(&self) -> usize {
        self.at
    }

    /// Its bytes from `from` to where it is.
    pub fn since(&self, from: usize) -> &'a [u8] {
        &self.data[from..self.at]
    }
}

/// What a binding of the pipeline's one descriptor set holds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Uniform {
    Sampler,
    Buffer,
    Texel(vk::Format),
}

/// A binding with the name the game knows it by (for the message when a draw leaves it empty).
pub struct Slot {
    pub name: String,
    pub kind: Uniform,
}

pub struct Pipeline {
    pub name: String,
    pub with_depth: vk::Pipeline,
    pub without_depth: vk::Pipeline,
    pub layout: vk::PipelineLayout,
    set_layout: vk::DescriptorSetLayout,
    modules: Vec<vk::ShaderModule>,
    pub uniforms: Vec<Slot>,
    variants: [vk::Pipeline; VARIANTS],
    /// The same pipeline with the game's own shaders (with and without depth), for frames the engine draws nothing in.
    game: [vk::Pipeline; 2],
    /// The bindings its vertex shaders declare images or texel buffers at (bit per binding): read before the
    /// fragments, so a texture drawn into just before needs a full barrier (pass.rs, reads_unsettled).
    pub vertex_samplers: u64,
    /// The sky's pipelines' variant for drawing it behind the opaque world (pass.rs, sky_behind): drawn at the depth
    /// the attachment was cleared to (the far one) and tested against it the way the world's depth goes, not written —
    /// greater or equal for the game's reversed depth (far at 0), less or equal for a far at 1; tested in the world's
    /// direction, the GPU rejects covered tiles whole (for equality it tests every pixel: 21 µs against 25 for the
    /// whole sky at 1440p, 12.10). Null for the rest.
    pub behind: [vk::Pipeline; 2],
    /// Opaque, depth tested and written: what may be drawn before the sky (pass.rs).
    pub opaque: bool,
    /// A creature's chams (cursed.recode.chams.Chams, named cursed-recode:chams/…): always its own shaders, the
    /// engine's or not, and the picture so far taken before the first of them in a pass (pass.rs).
    pub chams: bool,
}

/// The game's sky pipelines (SkyRenderer), whose draws can wait behind the opaque world (pass.rs, sky_behind).
const SKY_PIPELINES: [&str; 5] = ["pipeline/sky", "pipeline/sunrise_sunset", "pipeline/stars", "pipeline/celestial", "pipeline/end_sky"];

/// The bindings a SPIR-V module declares UniformConstant variables at (images, samplers, texel buffers), as bits.
fn constant_bindings(code: &[u32]) -> u64 {
    let mut bindings = std::collections::HashMap::new();
    let mut constants = Vec::new();
    let mut i = 5;
    while i < code.len() {
        let count = (code[i] >> 16) as usize;
        let op = code[i] & 0xffff;
        if count == 0 || i + count > code.len() {
            break;
        }
        // OpDecorate %target Binding n
        if op == 71 && count >= 4 && code[i + 2] == 33 {
            bindings.insert(code[i + 1], code[i + 3]);
        }
        // OpVariable %type %id UniformConstant
        if op == 59 && count >= 4 && code[i + 3] == 0 {
            constants.push(code[i + 2]);
        }
        i += count;
    }
    constants.iter().filter_map(|id| bindings.get(id)).filter(|&&b| b < 64).fold(0, |m, &b| m | 1u64 << b)
}

/// The engine's variants of a terrain pipeline (cursed.recode.render.EngineShaders writes the same numbers).
pub const PREPASS: usize = 0;
pub const PREPASS_CUTOUT: usize = 1;
pub const SHADE: usize = 2;
const VARIANTS: usize = 3;
/// The game's own vertex and fragment shaders of a pipeline the engine's replaced (cursed.recode.render.CursedPipeline).
const GAME_VERTEX: usize = 3;
const GAME_FRAGMENT: usize = 4;

fn check<T>(r: Result<T, vk::Result>, what: impl FnOnce() -> String) -> T {
    r.unwrap_or_else(|e| panic!("{}: {e:?}", what()))
}

impl Pipeline {
    pub fn compile(gpu: &Gpu, info: &[u8]) -> Pipeline {
        let d = &gpu.device;
        let mut r = Reader::new(info);
        let name = r.str();

        struct Shader {
            stage: vk::ShaderStageFlags,
            entry: std::ffi::CString,
            code: Vec<u32>,
        }
        let shader_count = r.i32();
        let mut shaders = Vec::new();
        for _ in 0..shader_count {
            let stage = vk::ShaderStageFlags::from_raw(r.u32());
            let entry = std::ffi::CString::new(r.str()).unwrap_or_else(|_| c"main".to_owned());
            let addr = r.i64() as usize as *const u8;
            let len = r.i32().max(0) as usize;
            let bytes = unsafe { std::slice::from_raw_parts(addr, len) };
            let code: Vec<u32> = bytes.chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect();
            shaders.push(Shader { stage, entry, code });
        }
        let mut vertex_samplers = shaders.iter().filter(|s| s.stage == vk::ShaderStageFlags::VERTEX).fold(0, |m, s| m | constant_bindings(&s.code));
        let mut bindings = Vec::new();
        let mut divisors = Vec::new();
        for _ in 0..r.i32() {
            let slot = r.u32();
            let stride = r.u32();
            let step = r.u32();
            bindings.push(
                vk::VertexInputBindingDescription::default()
                    .binding(slot)
                    .stride(stride)
                    .input_rate(if step > 0 { vk::VertexInputRate::INSTANCE } else { vk::VertexInputRate::VERTEX }),
            );
            if step > 0 {
                divisors.push(vk::VertexInputBindingDivisorDescriptionEXT { binding: slot, divisor: step });
            }
        }
        let mut attributes = Vec::new();
        for _ in 0..r.i32() {
            let binding = r.u32();
            let location = r.u32();
            let offset = r.u32();
            let format = vk::Format::from_raw(r.i32());
            attributes.push(vk::VertexInputAttributeDescription { location, binding, format, offset });
        }
        let mut uniforms = Vec::new();
        for _ in 0..r.i32() {
            let name = r.str();
            let kind = r.i32();
            let format = vk::Format::from_raw(r.i32());
            let kind = match kind {
                0 => Uniform::Sampler,
                1 => Uniform::Buffer,
                _ => Uniform::Texel(format),
            };
            uniforms.push(Slot { name, kind });
        }
        let push_size = r.u32();
        let has_depth = r.i32() != 0;
        let (compare, write_depth, bias_slope, bias_constant) = if has_depth {
            (vk::CompareOp::from_raw(r.i32()), r.i32() != 0, r.f32(), r.f32())
        } else {
            (vk::CompareOp::ALWAYS, false, 0.0, 0.0)
        };
        let polygon = vk::PolygonMode::from_raw(r.i32());
        let cull = r.i32() != 0;
        let mut blends = Vec::new();
        let mut formats = Vec::new();
        for _ in 0..r.i32() {
            let present = r.i32() != 0;
            let format = vk::Format::from_raw(r.i32());
            let mask = vk::ColorComponentFlags::from_raw(r.u32());
            let blend = r.i32() != 0;
            let f: Vec<i32> = (0..6).map(|_| r.i32()).collect();
            let mut state = vk::PipelineColorBlendAttachmentState::default().color_write_mask(if present { mask } else { vk::ColorComponentFlags::empty() });
            if present && blend {
                state = state
                    .blend_enable(true)
                    .src_color_blend_factor(vk::BlendFactor::from_raw(f[0]))
                    .dst_color_blend_factor(vk::BlendFactor::from_raw(f[1]))
                    .color_blend_op(vk::BlendOp::from_raw(f[2]))
                    .src_alpha_blend_factor(vk::BlendFactor::from_raw(f[3]))
                    .dst_alpha_blend_factor(vk::BlendFactor::from_raw(f[4]))
                    .alpha_blend_op(vk::BlendOp::from_raw(f[5]));
            }
            blends.push(state);
            formats.push(if present { format } else { vk::Format::UNDEFINED });
        }
        let topology = vk::PrimitiveTopology::from_raw(r.i32());

        let set_bindings: Vec<vk::DescriptorSetLayoutBinding> = uniforms
            .iter()
            .enumerate()
            .map(|(i, u)| {
                vk::DescriptorSetLayoutBinding::default()
                    .binding(i as u32)
                    .descriptor_type(match u.kind {
                        Uniform::Sampler => vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                        Uniform::Buffer => vk::DescriptorType::UNIFORM_BUFFER,
                        Uniform::Texel(_) => vk::DescriptorType::UNIFORM_TEXEL_BUFFER,
                    })
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
            })
            .collect();
        let set_info = vk::DescriptorSetLayoutCreateInfo::default()
            .flags(vk::DescriptorSetLayoutCreateFlags::PUSH_DESCRIPTOR_KHR)
            .bindings(&set_bindings);
        let set_layout = check(unsafe { d.create_descriptor_set_layout(&set_info, None) }, || format!("Can't create descriptor set layout for {name}"));
        let ranges = [vk::PushConstantRange { stage_flags: vk::ShaderStageFlags::ALL, offset: 0, size: push_size }];
        let set_layouts = [set_layout];
        let mut layout_info = vk::PipelineLayoutCreateInfo::default().set_layouts(&set_layouts);
        if push_size != 0 {
            layout_info = layout_info.push_constant_ranges(&ranges);
        }
        let layout = check(unsafe { d.create_pipeline_layout(&layout_info, None) }, || format!("Can't create pipeline for {name}"));
        gpu.name(layout, &format!("Pipeline layout for {name}"));

        let mut modules = Vec::new();
        let mut stages = Vec::new();
        for s in &shaders {
            let info = vk::ShaderModuleCreateInfo::default().code(&s.code);
            let module = check(unsafe { d.create_shader_module(&info, None) }, || format!("Can't compile {:?} shader for pipeline {name}", s.stage));
            gpu.name(module, &name);
            modules.push(module);
            stages.push(vk::PipelineShaderStageCreateInfo::default().stage(s.stage).module(module).name(&s.entry));
        }
        let mut divisor_state = vk::PipelineVertexInputDivisorStateCreateInfoEXT::default().vertex_binding_divisors(&divisors);
        let mut vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
            .vertex_binding_descriptions(&bindings)
            .vertex_attribute_descriptions(&attributes);
        if !divisors.is_empty() {
            vertex_input = vertex_input.push_next(&mut divisor_state);
        }
        let assembly = vk::PipelineInputAssemblyStateCreateInfo::default().topology(topology);
        let raster = vk::PipelineRasterizationStateCreateInfo::default()
            .polygon_mode(polygon)
            .cull_mode(if cull { vk::CullModeFlags::BACK } else { vk::CullModeFlags::NONE })
            .front_face(vk::FrontFace::CLOCKWISE)
            .line_width(1.0)
            .depth_bias_enable(has_depth && (bias_constant != 0.0 || bias_slope != 0.0))
            .depth_bias_constant_factor(bias_constant)
            .depth_bias_slope_factor(bias_slope);
        let depth_state = vk::PipelineDepthStencilStateCreateInfo::default()
            .depth_test_enable(has_depth)
            .depth_write_enable(write_depth)
            .depth_compare_op(compare);
        let blend = vk::PipelineColorBlendStateCreateInfo::default().attachments(&blends);
        let viewport = vk::PipelineViewportStateCreateInfo::default().viewport_count(1).scissor_count(1);
        let multisample = vk::PipelineMultisampleStateCreateInfo::default().rasterization_samples(vk::SampleCountFlags::TYPE_1);
        let dynamic_states = [vk::DynamicState::SCISSOR, vk::DynamicState::VIEWPORT];
        let dynamic = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

        let build = |stages: &[vk::PipelineShaderStageCreateInfo], depth_format: vk::Format| -> vk::Pipeline {
            let mut rendering = vk::PipelineRenderingCreateInfo::default().color_attachment_formats(&formats).depth_attachment_format(depth_format);
            let info = [vk::GraphicsPipelineCreateInfo::default()
                .stages(stages)
                .vertex_input_state(&vertex_input)
                .input_assembly_state(&assembly)
                .rasterization_state(&raster)
                .depth_stencil_state(&depth_state)
                .color_blend_state(&blend)
                .viewport_state(&viewport)
                .multisample_state(&multisample)
                .dynamic_state(&dynamic)
                .layout(layout)
                .push_next(&mut rendering)];
            let p = unsafe { d.create_graphics_pipelines(vk::PipelineCache::null(), &info, None) }
                .map(|v| v[0])
                .map_err(|(_, e)| e);
            let p = check(p, || format!("Can't compile pipeline {name}"));
            gpu.name(p, &format!("Pipeline {name}"));
            p
        };
        let with_depth = build(&stages, vk::Format::D32_SFLOAT);
        let without_depth = if has_depth { vk::Pipeline::null() } else { build(&stages, vk::Format::UNDEFINED) };
        let opaque = has_depth && write_depth && blends.iter().all(|b| b.blend_enable == vk::FALSE);
        let behind = if !has_depth && SKY_PIPELINES.iter().any(|s| name.ends_with(s)) {
            [vk::CompareOp::GREATER_OR_EQUAL, vk::CompareOp::LESS_OR_EQUAL].map(|compare| {
            let behind_depth = vk::PipelineDepthStencilStateCreateInfo::default().depth_test_enable(true).depth_write_enable(false).depth_compare_op(compare);
            let mut rendering = vk::PipelineRenderingCreateInfo::default().color_attachment_formats(&formats).depth_attachment_format(vk::Format::D32_SFLOAT);
            let info = [vk::GraphicsPipelineCreateInfo::default()
                .stages(&stages)
                .vertex_input_state(&vertex_input)
                .input_assembly_state(&assembly)
                .rasterization_state(&raster)
                .depth_stencil_state(&behind_depth)
                .color_blend_state(&blend)
                .viewport_state(&viewport)
                .multisample_state(&multisample)
                .dynamic_state(&dynamic)
                .layout(layout)
                .push_next(&mut rendering)];
            match unsafe { d.create_graphics_pipelines(vk::PipelineCache::null(), &info, None) } {
                Ok(v) => {
                    gpu.name(v[0], &format!("Pipeline {name} (behind the world)"));
                    v[0]
                }
                Err(_) => vk::Pipeline::null(),
            }
            })
        } else {
            [vk::Pipeline::null(); 2]
        };

        // The engine's variants of opaque terrain (cursed.recode.render.EngineShaders): the same pipeline with another
        // fragment shader and depth state — the depth pass (writes depth, no colour; solid, then cutout) and the lit
        // pass (tests the laid depth for equality, writes no depth).
        let mut variants = [vk::Pipeline::null(); VARIANTS];
        let mut game = [vk::Pipeline::null(); 2];
        let mut game_stages = [None, None];
        if !r.done() {
            let n = r.i32();
            let vertex = stages.iter().find(|s| s.stage == vk::ShaderStageFlags::VERTEX).copied();
            for _ in 0..n {
                let kind = r.i32() as usize;
                let addr = r.i64() as usize as *const u8;
                let len = r.i32().max(0) as usize;
                let bytes = unsafe { std::slice::from_raw_parts(addr, len) };
                let code: Vec<u32> = bytes.chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect();
                if kind == GAME_VERTEX {
                    vertex_samplers |= constant_bindings(&code);
                }
                if kind == GAME_VERTEX || kind == GAME_FRAGMENT {
                    let module = check(unsafe { d.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&code), None) }, || format!("Can't compile the game's shader of {name}"));
                    modules.push(module);
                    let stage = if kind == GAME_VERTEX { vk::ShaderStageFlags::VERTEX } else { vk::ShaderStageFlags::FRAGMENT };
                    game_stages[kind - GAME_VERTEX] = Some(vk::PipelineShaderStageCreateInfo::default().stage(stage).module(module).name(c"main"));
                    continue;
                }
                let (Some(vertex), true) = (vertex, kind < VARIANTS) else { continue };
                let module = check(unsafe { d.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&code), None) }, || format!("Can't compile an engine variant of {name}"));
                modules.push(module);
                let fragment = vk::PipelineShaderStageCreateInfo::default().stage(vk::ShaderStageFlags::FRAGMENT).module(module).name(c"main");
                let variant_stages = [vertex, fragment];
                let depth_pass = kind != SHADE;
                let variant_depth = vk::PipelineDepthStencilStateCreateInfo::default()
                    .depth_test_enable(true)
                    .depth_write_enable(depth_pass)
                    .depth_compare_op(if depth_pass { compare } else { vk::CompareOp::EQUAL });
                let silent: Vec<vk::PipelineColorBlendAttachmentState> =
                    blends.iter().map(|_| vk::PipelineColorBlendAttachmentState::default().color_write_mask(vk::ColorComponentFlags::empty())).collect();
                let variant_blend = vk::PipelineColorBlendStateCreateInfo::default().attachments(if depth_pass { &silent } else { &blends });
                let mut rendering = vk::PipelineRenderingCreateInfo::default().color_attachment_formats(&formats).depth_attachment_format(vk::Format::D32_SFLOAT);
                let info = [vk::GraphicsPipelineCreateInfo::default()
                    .stages(&variant_stages)
                    .vertex_input_state(&vertex_input)
                    .input_assembly_state(&assembly)
                    .rasterization_state(&raster)
                    .depth_stencil_state(&variant_depth)
                    .color_blend_state(&variant_blend)
                    .viewport_state(&viewport)
                    .multisample_state(&multisample)
                    .dynamic_state(&dynamic)
                    .layout(layout)
                    .push_next(&mut rendering)];
                let p = unsafe { d.create_graphics_pipelines(vk::PipelineCache::null(), &info, None) }.map(|v| v[0]).map_err(|(_, e)| e);
                let p = check(p, || format!("Can't compile an engine variant of {name}"));
                gpu.name(p, &format!("Pipeline {name} (engine {kind})"));
                variants[kind] = p;
            }
        }
        if let [Some(v), Some(f)] = game_stages {
            game[0] = build(&[v, f], vk::Format::D32_SFLOAT);
            gpu.name(game[0], &format!("Pipeline {name} (game's shaders)"));
            if !has_depth {
                game[1] = build(&[v, f], vk::Format::UNDEFINED);
            }
        }
        if std::env::var_os("CURSED_DEV_VERTEXREADS").is_some() && vertex_samplers != 0 {
            let names: Vec<&str> = uniforms.iter().enumerate().filter(|(i, _)| *i < 64 && vertex_samplers & (1 << i) != 0).map(|(_, u)| u.name.as_str()).collect();
            eprintln!("[barriers] {name}: vertex shaders read {names:?}");
        }
        let chams = name.starts_with("cursed-recode:chams/");
        Pipeline { name, with_depth, without_depth, layout, set_layout, modules, uniforms, variants, game, vertex_samplers, behind, opaque, chams }
    }

    /// What a pass binds: with or without depth, and the game's own shaders when the engine draws nothing (`vanilla`).
    pub fn handle(&self, depth: bool, vanilla: bool) -> vk::Pipeline {
        let game = self.game[if depth { 0 } else { 1 }];
        if vanilla && !self.chams && game != vk::Pipeline::null() {
            return game;
        }
        if depth { self.with_depth } else { self.without_depth }
    }

    /// The engine variant `kind` (PREPASS, PREPASS_CUTOUT, SHADE), if this pipeline has it.
    pub fn variant(&self, kind: usize) -> Option<vk::Pipeline> {
        let p = self.variants[kind];
        (p != vk::Pipeline::null()).then_some(p)
    }

    pub fn destroy(&self, gpu: &Gpu) {
        let d = &gpu.device;
        unsafe {
            if self.without_depth != vk::Pipeline::null() {
                d.destroy_pipeline(self.without_depth, None);
            }
            for &v in self.variants.iter().chain(&self.game).chain(&self.behind) {
                if v != vk::Pipeline::null() {
                    d.destroy_pipeline(v, None);
                }
            }
            d.destroy_pipeline(self.with_depth, None);
            d.destroy_pipeline_layout(self.layout, None);
            d.destroy_descriptor_set_layout(self.set_layout, None);
            for &m in &self.modules {
                d.destroy_shader_module(m, None);
            }
        }
    }
}
