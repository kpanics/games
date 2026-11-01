//! Memory that lives for one frame: the vertices of the GUI, the uniforms of a draw, what is on its way to a texture.
//!
//! The same scheme as Mojang's (backend/util/TransientBlockAllocator and backend/vulkan/VulkanTransientMemory): blocks
//! of 512 KiB handed out by bumping an offset, given back whole when the frame that used them is done on the GPU.
//! Four kinds:
//! - cpu: plain memory of the process, for the CPU only;
//! - staging: memory the CPU writes and the GPU copies from;
//! - gpu: video memory the GPU alone touches;
//! - mapped: memory the CPU writes and the GPU reads as if it were video memory. With Resizable BAR it is video
//!   memory the CPU sees; without, a staging block whose whole content is copied to a gpu block at the start of the
//!   frame's submit (the copy is listed the moment the block is first used; the frame thread records the list into
//!   a command buffer that runs first in the submit).
//!
//! Two of these exist: the game's (its thread asks for pieces while it builds a frame) and the frame thread's own
//! (terrain uploads). Each rotates with the frames of its own thread.

use super::device::Gpu;
use super::memory::Place;
use super::resources::{Buffer, Doomed};
use ash::vk;
use std::collections::VecDeque;

pub const BLOCK: u64 = 512 * 1024;
const CPU_MAX_ALIGN: u64 = 16;

fn round_up(v: u64, align: u64) -> u64 {
    let a = align.max(1);
    v.div_ceil(a) * a
}

/// A bump allocator over blocks of one kind; `B` is the block.
struct Blocks<B> {
    size: u64,
    max_align: u64,
    special: Vec<Box<B>>,
    free: Vec<Box<B>>,
    used: Vec<Box<B>>,
    current: Option<*const B>,
    offset: u64,
}

impl<B> Blocks<B> {
    fn new(size: u64, max_align: u64) -> Self {
        Blocks { size, max_align, special: Vec::new(), free: Vec::new(), used: Vec::new(), current: None, offset: size }
    }

    fn fits_block(&self, size: u64, align: u64) -> bool {
        size <= self.size && align <= self.max_align
    }

    fn fits_current(&self, size: u64, align: u64) -> bool {
        if self.current.is_none() && self.fits_block(size, align) {
            return true;
        }
        let at = round_up(self.offset, align);
        at <= self.size && size <= self.size - at && align <= self.max_align
    }

    fn take_block(&mut self, make: &mut dyn FnMut(u64) -> Box<B>, on_use: &mut dyn FnMut(&B)) -> *const B {
        let b = match self.free.pop() {
            Some(b) => b,
            None => make(self.size),
        };
        on_use(&b);
        let p = &*b as *const B;
        self.used.push(b);
        p
    }

    /// A piece: its block, where in the block, and how long. When `size` does not fit what is left of the block but
    /// `minimum` does, the piece is what is left, cut to whole `element`s (the game then writes in parts).
    fn allocate(
        &mut self,
        size: u64,
        align: u64,
        minimum: u64,
        element: u64,
        make: &mut dyn FnMut(u64) -> Box<B>,
        on_use: &mut dyn FnMut(&B),
    ) -> (*const B, u64, u64) {
        assert!(align <= self.max_align, "Alignment requirement over maximum supported alignment");
        if size == self.size {
            return (self.take_block(make, on_use), 0, self.size);
        }
        if !self.fits_block(size, align) {
            let b = make(size);
            on_use(&b);
            let p = &*b as *const B;
            self.special.push(b);
            return (p, 0, size);
        }
        if self.current.is_none() {
            self.current = Some(self.take_block(make, on_use));
            self.offset = 0;
        }
        let current = self.current.unwrap();
        if self.fits_current(size, align) {
            let at = round_up(self.offset, align);
            self.offset = at + size;
            return (current, at, size);
        }
        if self.fits_current(minimum, align) {
            let at = round_up(self.offset, align);
            let element = element.max(1);
            let n = (self.size - at) / element * element;
            self.offset = at + n;
            return (current, at, n);
        }
        let b = self.take_block(make, on_use);
        if self.offset > size {
            self.current = Some(b);
            self.offset = size;
        }
        (b, 0, size)
    }

    /// Ends a frame: what was free all frame long is returned to be freed, what was used this frame is returned to be
    /// recycled once the GPU is done with it.
    fn rotate(&mut self) -> (Vec<Box<B>>, Vec<Box<B>>, Vec<Box<B>>) {
        self.current = None;
        self.offset = self.size;
        (std::mem::take(&mut self.free), std::mem::take(&mut self.used), std::mem::take(&mut self.special))
    }
}

struct CpuBlock {
    mem: Vec<u128>,
}

impl CpuBlock {
    fn ptr(&self) -> *mut u8 {
        self.mem.as_ptr() as *mut u8
    }
}

pub struct GpuBlock {
    pub buffer: Buffer,
    suboptimal: bool,
}

/// A block of the mapped kind: where the CPU writes, and where the GPU reads (the same block with Resizable BAR).
struct Pair {
    cpu: *const GpuBlock,
    gpu: *const GpuBlock,
}

/// The blocks of one kind given up at a submit, waiting for that submit to finish on the GPU.
struct Retired {
    submit: u64,
    staging: (Vec<Box<GpuBlock>>, Vec<Box<GpuBlock>>),
    gpu: Option<(Vec<Box<GpuBlock>>, Vec<Box<GpuBlock>>)>,
}

/// A piece of transient memory, as Java gets it.
#[derive(Clone, Copy, Default)]
pub struct Piece {
    /// The `Buffer` of the block (null for cpu pieces).
    pub buffer: *const Buffer,
    pub offset: u64,
    pub size: u64,
    /// Where the CPU writes it (null for gpu pieces).
    pub host: *mut u8,
}

pub struct Transient {
    rebar: bool,
    cpu: Blocks<CpuBlock>,
    staging: Blocks<GpuBlock>,
    gpu: Blocks<GpuBlock>,
    mapped: Blocks<Pair>,
    retired: VecDeque<Retired>,
    /// The whole-block copies of mapped pairs used this frame: source, destination, size.
    copies: Vec<(vk::Buffer, vk::Buffer, u64)>,
}

unsafe impl Send for Transient {}

fn make_block(gpu: &Gpu, size: u64, place: Place) -> Box<GpuBlock> {
    let flags = vk::BufferUsageFlags::TRANSFER_SRC
        | vk::BufferUsageFlags::TRANSFER_DST
        | vk::BufferUsageFlags::UNIFORM_TEXEL_BUFFER
        | vk::BufferUsageFlags::UNIFORM_BUFFER
        | vk::BufferUsageFlags::INDEX_BUFFER
        | vk::BufferUsageFlags::VERTEX_BUFFER
        | vk::BufferUsageFlags::INDIRECT_BUFFER
        | vk::BufferUsageFlags::STORAGE_BUFFER;
    let buffer = Buffer::with(gpu, flags, size, place).unwrap_or_else(|e| panic!("Failed to allocate VkBuffer: {e:?}"));
    gpu.name(buffer.raw, "Transient Memory Buffer");
    let suboptimal = buffer.alloc.as_ref().is_some_and(|a| a.fallback);
    Box::new(GpuBlock { buffer, suboptimal })
}

fn free_block(gpu: &Gpu, b: Box<GpuBlock>) {
    Doomed::Buffer(Box::new(b.buffer)).destroy(gpu);
}

impl Transient {
    pub fn new(gpu: &Gpu) -> Self {
        Transient {
            rebar: gpu.memory.rebar,
            cpu: Blocks::new(BLOCK, CPU_MAX_ALIGN),
            staging: Blocks::new(BLOCK, u64::MAX / 2 + 1),
            gpu: Blocks::new(BLOCK, u64::MAX / 2 + 1),
            mapped: Blocks::new(BLOCK, u64::MAX / 2 + 1),
            retired: VecDeque::new(),
            copies: Vec::new(),
        }
    }

    fn staging_place(&self) -> Place {
        if self.rebar {
            Place::Shared
        } else {
            Place::Upload
        }
    }

    fn gpu_place(&self) -> Place {
        if self.rebar {
            Place::Shared
        } else {
            Place::Device
        }
    }

    /// The copies the frame's mapped pieces need, taken for recording.
    pub fn take_copies(&mut self) -> Vec<(vk::Buffer, vk::Buffer, u64)> {
        std::mem::take(&mut self.copies)
    }

    /// Ends the frame's transient memory at submit `submit` and gives the gpu blocks to free to `doomed` (they wait
    /// two submits there, like everything else). Take the copies first.
    pub fn end(&mut self, gpu: &Gpu, submit: u64, doomed: &mut Vec<Doomed>) {
        // cpu blocks: back to the pool at once, the GPU never sees them; the ones free all frame long are dropped.
        let (_stale, used, _special) = self.cpu.rotate();
        self.cpu.free.extend(used);
        let (stale, used, special) = self.staging.rotate();
        for b in stale {
            free_block(gpu, b);
        }
        let mut retired = Retired { submit, staging: (used, special), gpu: None };
        let (stale, used, special) = self.gpu.rotate();
        if self.rebar {
            for b in stale {
                free_block(gpu, b);
            }
            retired.gpu = Some((used, special));
        } else {
            // Video memory only the GPU writes: the copies into it come after this frame's reads on the same queue.
            for b in stale {
                doomed.push(Doomed::Buffer(Box::new(b.buffer)));
            }
            for b in used {
                if b.suboptimal {
                    doomed.push(Doomed::Buffer(Box::new(b.buffer)));
                } else {
                    self.gpu.free.push(b);
                }
            }
            for b in special {
                doomed.push(Doomed::Buffer(Box::new(b.buffer)));
            }
        }
        self.retired.push_back(retired);
        // Pairs point into the staging and gpu blocks, which those two kinds give back themselves.
        let _ = self.mapped.rotate();
    }

    /// Blocks given up at submits the GPU has finished go back to their kind.
    pub fn recycle(&mut self, gpu: &Gpu, completed: u64) {
        while self.retired.front().is_some_and(|r| r.submit <= completed) {
            let r = self.retired.pop_front().unwrap();
            let (used, special) = r.staging;
            for b in used {
                if b.suboptimal {
                    free_block(gpu, b);
                } else {
                    self.staging.free.push(b);
                }
            }
            for b in special {
                free_block(gpu, b);
            }
            if let Some((used, special)) = r.gpu {
                for b in used {
                    if b.suboptimal {
                        free_block(gpu, b);
                    } else {
                        self.gpu.free.push(b);
                    }
                }
                for b in special {
                    free_block(gpu, b);
                }
            }
        }
    }

    pub fn cpu(&mut self, size: u64, align: u64, minimum: u64, element: u64) -> Piece {
        let (b, at, n) = self.cpu.allocate(
            size,
            align,
            minimum,
            element,
            &mut |s| Box::new(CpuBlock { mem: vec![0u128; s.div_ceil(16) as usize] }),
            &mut |_| {},
        );
        Piece { buffer: std::ptr::null(), offset: 0, size: n, host: unsafe { (*b).ptr().add(at as usize) } }
    }

    pub fn staging(&mut self, gpu: &Gpu, size: u64, align: u64, minimum: u64, element: u64) -> Piece {
        let place = self.staging_place();
        let (b, at, n) = self.staging.allocate(size, align, minimum, element, &mut |s| make_block(gpu, s, place), &mut |_| {});
        let b = unsafe { &*b };
        Piece { buffer: &b.buffer, offset: at, size: n, host: unsafe { b.buffer.ptr().add(at as usize) } }
    }

    pub fn gpu(&mut self, gpu: &Gpu, size: u64, align: u64, minimum: u64, element: u64) -> Piece {
        let place = self.gpu_place();
        let (b, at, n) = self.gpu.allocate(size, align, minimum, element, &mut |s| make_block(gpu, s, place), &mut |_| {});
        let b = unsafe { &*b };
        Piece { buffer: &b.buffer, offset: at, size: n, host: std::ptr::null_mut() }
    }

    pub fn mapped(&mut self, gpu: &Gpu, size: u64, align: u64, minimum: u64, element: u64) -> Piece {
        let rebar = self.rebar;
        let staging_place = self.staging_place();
        let gpu_place = self.gpu_place();
        let staging = &mut self.staging;
        let gpu_blocks = &mut self.gpu;
        let copies = &mut self.copies;
        let mut make = |s: u64| -> Box<Pair> {
            debug_assert!(s >= BLOCK);
            if rebar {
                let (g, _, _) = gpu_blocks.allocate(s, 16, s, 1, &mut |n| make_block(gpu, n, gpu_place), &mut |_| {});
                Box::new(Pair { cpu: g, gpu: g })
            } else {
                let (c, _, _) = staging.allocate(s, 16, s, 1, &mut |n| make_block(gpu, n, staging_place), &mut |_| {});
                let (g, _, _) = gpu_blocks.allocate(s, 16, s, 1, &mut |n| make_block(gpu, n, gpu_place), &mut |_| {});
                Box::new(Pair { cpu: c, gpu: g })
            }
        };
        let mut on_use = |p: &Pair| {
            if p.cpu != p.gpu {
                let (c, g) = unsafe { (&*p.cpu, &*p.gpu) };
                copies.push((c.buffer.raw, g.buffer.raw, c.buffer.size));
            }
        };
        let (p, at, n) = self.mapped.allocate(size, align, minimum, element, &mut make, &mut on_use);
        let p = unsafe { &*p };
        let (c, g) = unsafe { (&*p.cpu, &*p.gpu) };
        Piece { buffer: &g.buffer, offset: at, size: n, host: unsafe { c.buffer.ptr().add(at as usize) } }
    }

    /// Whether a piece fits in what is left of the current block (Mojang's multiUpload packs by it).
    pub fn fits(&self, mapped: bool, size: u64, align: u64) -> bool {
        if mapped {
            self.mapped.fits_current(size, align)
        } else {
            self.staging.fits_current(size, align)
        }
    }

    pub fn destroy(&mut self, gpu: &Gpu) {
        self.recycle(gpu, u64::MAX);
        for kind in [&mut self.staging, &mut self.gpu] {
            let (stale, used, special) = kind.rotate();
            for b in stale.into_iter().chain(used).chain(special) {
                free_block(gpu, b);
            }
        }
        let _ = self.cpu.rotate();
        let _ = self.mapped.rotate();
    }
}
