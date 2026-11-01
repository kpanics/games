//! The game's objects on the GPU: buffers, textures, their views, samplers, timestamp pools, and what is left of them
//! until the GPU is done.
//!
//! Java holds each one as the address of its Rust struct. Closing one never frees it on the spot: it goes to the
//! destruction queue (encoder.rs) and is freed two submits later, when no frame in flight can still use it.

use super::device::Gpu;
use super::memory::{Allocation, Place};
use ash::vk;
use std::sync::atomic::{AtomicBool, AtomicU32};

// The game's usage bits (GpuBuffer.USAGE_*, GpuTexture.USAGE_*).
const BUF_MAP_READ: i32 = 1;
const BUF_MAP_WRITE: i32 = 2;
const BUF_CLIENT_STORAGE: i32 = 4;
const BUF_COPY_DST: i32 = 8;
const BUF_COPY_SRC: i32 = 16;
const BUF_VERTEX: i32 = 32;
const BUF_INDEX: i32 = 64;
const BUF_UNIFORM: i32 = 128;
const BUF_TEXEL: i32 = 256;
const BUF_INDIRECT: i32 = 512;

const TEX_COPY_DST: i32 = 1;
const TEX_COPY_SRC: i32 = 2;
const TEX_BINDING: i32 = 4;
const TEX_ATTACHMENT: i32 = 8;
const TEX_CUBE: i32 = 16;

pub fn buffer_usage(usage: i32) -> vk::BufferUsageFlags {
    let mut f = vk::BufferUsageFlags::empty();
    let pairs = [
        (BUF_COPY_DST, vk::BufferUsageFlags::TRANSFER_DST),
        (BUF_COPY_SRC, vk::BufferUsageFlags::TRANSFER_SRC),
        (BUF_VERTEX, vk::BufferUsageFlags::VERTEX_BUFFER),
        (BUF_INDEX, vk::BufferUsageFlags::INDEX_BUFFER),
        (BUF_UNIFORM, vk::BufferUsageFlags::UNIFORM_BUFFER),
        (BUF_TEXEL, vk::BufferUsageFlags::UNIFORM_TEXEL_BUFFER),
        (BUF_INDIRECT, vk::BufferUsageFlags::INDIRECT_BUFFER),
    ];
    for (bit, flag) in pairs {
        if usage & bit != 0 {
            f |= flag;
        }
    }
    // Vertex buffers can be written by compute shaders (vk/models.rs fills entity vertices in the game's buffers).
    if usage & BUF_VERTEX != 0 {
        f |= vk::BufferUsageFlags::STORAGE_BUFFER;
    }
    f
}

pub struct Buffer {
    pub raw: vk::Buffer,
    pub size: u64,
    pub alloc: Option<Allocation>,
}

impl Buffer {
    pub fn new(gpu: &Gpu, usage: i32, size: u64) -> Result<Buffer, vk::Result> {
        // The same places VMA picks for Mojang (VulkanGpuBuffer.Direct): read back → host, kept by the client →
        // host, written by the CPU → video memory the CPU sees, the rest → video memory.
        let place = if usage & BUF_MAP_READ != 0 {
            Place::Readback
        } else if usage & BUF_CLIENT_STORAGE != 0 {
            Place::Upload
        } else if usage & BUF_MAP_WRITE != 0 {
            Place::Shared
        } else {
            Place::Device
        };
        let mut flags = buffer_usage(usage);
        if flags.is_empty() {
            flags = vk::BufferUsageFlags::TRANSFER_DST;
        }
        Self::with(gpu, flags, size, place)
    }

    pub fn with(gpu: &Gpu, flags: vk::BufferUsageFlags, size: u64, place: Place) -> Result<Buffer, vk::Result> {
        let info = vk::BufferCreateInfo::default().size(size).usage(flags).sharing_mode(vk::SharingMode::EXCLUSIVE);
        let raw = unsafe { gpu.device.create_buffer(&info, None)? };
        let req = unsafe { gpu.device.get_buffer_memory_requirements(raw) };
        let alloc = match gpu.memory.allocate(&gpu.device, req, place, true) {
            Ok(a) => a,
            Err(e) => {
                unsafe { gpu.device.destroy_buffer(raw, None) };
                return Err(e);
            }
        };
        if let Err(e) = unsafe { gpu.device.bind_buffer_memory(raw, alloc.memory, alloc.offset) } {
            unsafe { gpu.device.destroy_buffer(raw, None) };
            gpu.memory.free(&gpu.device, alloc);
            return Err(e);
        }
        Ok(Buffer { raw, size, alloc: Some(alloc) })
    }

    /// Where the CPU sees the buffer's start; null for memory only the GPU sees.
    pub fn ptr(&self) -> *mut u8 {
        self.alloc.as_ref().map_or(std::ptr::null_mut(), |a| a.ptr)
    }

    /// The CPU sees it through its caches (reading it back and writing it piece by piece cost what plain memory costs).
    pub fn cached(&self) -> bool {
        self.alloc.as_ref().is_some_and(|a| !a.ptr.is_null() && a.cached)
    }
}

pub struct Texture {
    pub image: vk::Image,
    pub alloc: Option<Allocation>,
    pub format: vk::Format,
    pub aspect: vk::ImageAspectFlags,
    pub width: u32,
    pub height: u32,
    pub layers: u32,
    pub mips: u32,
    pub cube: bool,
    pub closed: AtomicBool,
    /// The texture itself counts as one; it goes when it is closed and the last view is closed. Views are made on the
    /// game's thread and closed on the frame thread, hence atomics.
    pub views: AtomicU32,
}

impl Texture {
    #[allow(clippy::too_many_arguments)]
    pub fn new(gpu: &Gpu, usage: i32, format: vk::Format, aspect: vk::ImageAspectFlags, width: u32, height: u32, layers: u32, mips: u32) -> Result<Texture, vk::Result> {
        let mut flags = vk::ImageUsageFlags::empty();
        if usage & TEX_ATTACHMENT != 0 {
            if aspect.contains(vk::ImageAspectFlags::COLOR) {
                flags |= vk::ImageUsageFlags::COLOR_ATTACHMENT;
            }
            if aspect.intersects(vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL) {
                flags |= vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT;
            }
        }
        if usage & TEX_BINDING != 0 {
            flags |= vk::ImageUsageFlags::SAMPLED;
        }
        if usage & TEX_COPY_DST != 0 {
            flags |= vk::ImageUsageFlags::TRANSFER_DST;
        }
        if usage & TEX_COPY_SRC != 0 {
            flags |= vk::ImageUsageFlags::TRANSFER_SRC;
        }
        let cube = usage & TEX_CUBE != 0;
        let info = vk::ImageCreateInfo::default()
            .flags(if cube { vk::ImageCreateFlags::CUBE_COMPATIBLE } else { vk::ImageCreateFlags::empty() })
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D { width, height, depth: 1 })
            .mip_levels(mips)
            .array_layers(layers)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(flags)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        let image = unsafe { gpu.device.create_image(&info, None)? };
        let req = unsafe { gpu.device.get_image_memory_requirements(image) };
        let alloc = match gpu.memory.allocate(&gpu.device, req, Place::Device, false) {
            Ok(a) => a,
            Err(e) => {
                unsafe { gpu.device.destroy_image(image, None) };
                return Err(e);
            }
        };
        if let Err(e) = unsafe { gpu.device.bind_image_memory(image, alloc.memory, alloc.offset) } {
            unsafe { gpu.device.destroy_image(image, None) };
            gpu.memory.free(&gpu.device, alloc);
            return Err(e);
        }
        Ok(Texture {
            image,
            alloc: Some(alloc),
            format,
            aspect,
            width,
            height,
            layers,
            mips,
            cube,
            closed: AtomicBool::new(false),
            views: AtomicU32::new(1),
        })
    }

    /// The aspect a view of it shows: colour, else depth, else stencil (a sampled view has one aspect).
    pub fn view_aspect(&self) -> vk::ImageAspectFlags {
        if self.aspect.contains(vk::ImageAspectFlags::COLOR) {
            vk::ImageAspectFlags::COLOR
        } else if self.aspect.contains(vk::ImageAspectFlags::DEPTH) {
            vk::ImageAspectFlags::DEPTH
        } else {
            vk::ImageAspectFlags::STENCIL
        }
    }

    pub fn width_at(&self, mip: u32) -> u32 {
        (self.width >> mip).max(1)
    }

    pub fn height_at(&self, mip: u32) -> u32 {
        (self.height >> mip).max(1)
    }
}

pub struct View {
    pub raw: vk::ImageView,
    pub texture: *const Texture,
    pub base_mip: u32,
}

impl View {
    pub fn new(gpu: &Gpu, texture: *const Texture, base_mip: u32, mips: u32) -> Result<View, vk::Result> {
        let t = unsafe { &*texture };
        let info = vk::ImageViewCreateInfo::default()
            .image(t.image)
            .view_type(if t.cube { vk::ImageViewType::CUBE } else { vk::ImageViewType::TYPE_2D })
            .format(t.format)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: t.view_aspect(),
                base_mip_level: base_mip,
                level_count: mips,
                base_array_layer: 0,
                layer_count: if t.cube { 6 } else { 1 },
            });
        let raw = unsafe { gpu.device.create_image_view(&info, None)? };
        t.views.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        Ok(View { raw, texture, base_mip })
    }

    pub fn texture(&self) -> &Texture {
        unsafe { &*self.texture }
    }
}

pub struct Sampler {
    pub raw: vk::Sampler,
}

impl Sampler {
    /// Address modes and filters come as the game's ordinals: REPEAT/CLAMP_TO_EDGE, NEAREST/LINEAR.
    pub fn new(gpu: &Gpu, address_u: i32, address_v: i32, min: i32, mag: i32, anisotropy: i32, max_lod: Option<f32>) -> Result<Sampler, vk::Result> {
        let address = |a: i32| if a == 0 { vk::SamplerAddressMode::REPEAT } else { vk::SamplerAddressMode::CLAMP_TO_EDGE };
        let filter = |f: i32| if f == 0 { vk::Filter::NEAREST } else { vk::Filter::LINEAR };
        let lod = max_lod.unwrap_or(1000.0);
        let info = vk::SamplerCreateInfo::default()
            .mag_filter(filter(mag))
            .min_filter(filter(min))
            .mipmap_mode(if lod > 0.25 { vk::SamplerMipmapMode::LINEAR } else { vk::SamplerMipmapMode::NEAREST })
            .address_mode_u(address(address_u))
            .address_mode_v(address(address_v))
            .address_mode_w(vk::SamplerAddressMode::REPEAT)
            .mip_lod_bias(0.0)
            .max_lod(lod.max(0.25))
            .anisotropy_enable(anisotropy > 1)
            .max_anisotropy(anisotropy as f32);
        Ok(Sampler { raw: unsafe { gpu.device.create_sampler(&info, None)? } })
    }
}

pub struct Queries {
    pub raw: vk::QueryPool,
    pub size: u32,
}

impl Queries {
    pub fn new(gpu: &Gpu, size: u32) -> Result<Queries, vk::Result> {
        let info = vk::QueryPoolCreateInfo::default().query_type(vk::QueryType::TIMESTAMP).query_count(size);
        let raw = unsafe { gpu.device.create_query_pool(&info, None)? };
        unsafe { gpu.device.reset_query_pool(raw, 0, size) };
        Ok(Queries { raw, size })
    }

    /// Two numbers per query: the value, and whether there is one yet.
    pub fn read(&self, gpu: &Gpu, first: u32, out: &mut [[u64; 2]]) -> Result<(), vk::Result> {
        let r = unsafe {
            gpu.device.get_query_pool_results(
                self.raw,
                first,
                out,
                vk::QueryResultFlags::TYPE_64 | vk::QueryResultFlags::WITH_AVAILABILITY,
            )
        };
        match r {
            Ok(()) | Err(vk::Result::NOT_READY) => Ok(()),
            Err(e) => Err(e),
        }
    }
}

/// What waits in the destruction queue.
pub enum Doomed {
    Buffer(Box<Buffer>),
    Texture(Box<Texture>),
    View(Box<View>),
    Sampler(Box<Sampler>),
    Queries(Box<Queries>),
    BufferView(vk::BufferView),
    Pipeline(Box<super::pipeline::Pipeline>),
}

impl Doomed {
    pub fn destroy(self, gpu: &Gpu) {
        let d = &gpu.device;
        unsafe {
            match self {
                Doomed::Buffer(mut b) => {
                    d.destroy_buffer(b.raw, None);
                    if let Some(a) = b.alloc.take() {
                        gpu.memory.free(d, a);
                    }
                }
                Doomed::Texture(mut t) => {
                    d.destroy_image(t.image, None);
                    if let Some(a) = t.alloc.take() {
                        gpu.memory.free(d, a);
                    }
                }
                Doomed::View(v) => d.destroy_image_view(v.raw, None),
                Doomed::Sampler(s) => d.destroy_sampler(s.raw, None),
                Doomed::Queries(q) => d.destroy_query_pool(q.raw, None),
                Doomed::BufferView(v) => d.destroy_buffer_view(v, None),
                Doomed::Pipeline(p) => p.destroy(gpu),
            }
        }
    }
}
