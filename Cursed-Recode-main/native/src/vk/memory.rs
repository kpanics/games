//! Video memory: big blocks taken from the driver, cut into the pieces buffers and images ask for.
//!
//! A driver gives out a few thousand allocations at most and each one is slow, so the game's thousands of buffers and
//! textures live inside blocks of 64 MiB. Buffers and images never share a block: the GPU's
//! `bufferImageGranularity` rule is about linear and optimal resources sitting next to each other, and with separate
//! blocks it can never apply. A piece bigger than half a block gets memory of its own.
//!
//! Which memory a piece goes to is the same choice Mojang makes through VMA (backend/vulkan in the game), written out
//! here as `Place`, so a texture lands where it lands for them and the frame costs what it costs for them.

use ash::vk;
use std::sync::Mutex;

/// What a piece of memory is for: decides the memory type.
static BIG: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static BIG_BYTES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Place {
    /// Only the GPU touches it: textures, vertex and index buffers.
    Device,
    /// The CPU writes it, the GPU copies from it: staging. Kept off the video memory.
    Upload,
    /// The CPU writes it and the GPU reads it where it is: video memory the CPU can see if there is any (Resizable BAR,
    /// the BAR window, an integrated GPU), the host's memory otherwise.
    Shared,
    /// The GPU writes it, the CPU reads it back: the host's memory, cached where the driver has such.
    Readback,
}

pub struct Allocation {
    pub memory: vk::DeviceMemory,
    pub offset: u64,
    pub size: u64,
    /// Where the CPU sees the piece; null when the memory is not mapped.
    pub ptr: *mut u8,
    pub type_index: u32,
    linear: bool,
    /// The block in its pool, or `None` for memory of its own.
    block: Option<usize>,
    /// The best memory type for its place was full and it went to the next one: worth giving back soon.
    pub fallback: bool,
    /// The CPU's reads and writes of it go through its caches (HOST_CACHED).
    pub cached: bool,
}

unsafe impl Send for Allocation {}
unsafe impl Sync for Allocation {}

const BLOCK: u64 = 64 << 20;

/// One block's free ranges, sorted by offset and never touching each other.
pub(crate) struct Ranges {
    size: u64,
    free: Vec<(u64, u64)>,
}

impl Ranges {
    pub(crate) fn new(size: u64) -> Self {
        Ranges { size, free: vec![(0, size)] }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.free.len() == 1 && self.free[0] == (0, self.size)
    }

    /// The first free range the piece fits in, aligned. Returns the piece's offset.
    pub(crate) fn take(&mut self, size: u64, align: u64) -> Option<u64> {
        let align = align.max(1);
        for i in 0..self.free.len() {
            let (start, len) = self.free[i];
            let at = start.div_ceil(align) * align;
            let lead = at - start;
            if lead + size > len {
                continue;
            }
            let tail = len - lead - size;
            match (lead > 0, tail > 0) {
                (false, false) => {
                    self.free.remove(i);
                }
                (true, false) => self.free[i] = (start, lead),
                (false, true) => self.free[i] = (at + size, tail),
                (true, true) => {
                    self.free[i] = (start, lead);
                    self.free.insert(i + 1, (at + size, tail));
                }
            }
            return Some(at);
        }
        None
    }

    /// Gives a piece back, joining it with free neighbours.
    pub(crate) fn give(&mut self, at: u64, size: u64) {
        let i = self.free.partition_point(|&(start, _)| start < at);
        debug_assert!(i == 0 || self.free[i - 1].0 + self.free[i - 1].1 <= at, "a piece given back twice");
        debug_assert!(i == self.free.len() || at + size <= self.free[i].0, "a piece given back twice");
        let joins_before = i > 0 && self.free[i - 1].0 + self.free[i - 1].1 == at;
        let joins_after = i < self.free.len() && at + size == self.free[i].0;
        match (joins_before, joins_after) {
            (true, true) => {
                self.free[i - 1].1 += size + self.free[i].1;
                self.free.remove(i);
            }
            (true, false) => self.free[i - 1].1 += size,
            (false, true) => self.free[i] = (at, size + self.free[i].1),
            (false, false) => self.free.insert(i, (at, size)),
        }
    }
}

struct Block {
    memory: vk::DeviceMemory,
    ptr: *mut u8,
    ranges: Ranges,
}

#[derive(Default)]
struct Pool {
    blocks: Vec<Option<Block>>,
}

struct Inner {
    /// One pool per memory type for buffers, one for images: `type * 2 + linear`.
    pools: Vec<Pool>,
}

unsafe impl Send for Inner {}

pub struct Memory {
    /// Bytes in allocations of their own (not in blocks), per memory type: for the dev line of what is held.
    dedicated: [std::sync::atomic::AtomicU64; 32],
    props: vk::PhysicalDeviceMemoryProperties,
    /// The biggest heap that is video memory has a type the CPU can see: Resizable BAR or an integrated GPU.
    pub rebar: bool,
    inner: Mutex<Inner>,
}

impl Memory {
    pub fn new(instance: &ash::Instance, physical: vk::PhysicalDevice) -> Self {
        let props = unsafe { instance.get_physical_device_memory_properties(physical) };
        let heaps = &props.memory_heaps[..props.memory_heap_count as usize];
        let types = &props.memory_types[..props.memory_type_count as usize];
        let mut video: Option<usize> = None;
        for (i, heap) in heaps.iter().enumerate() {
            if heap.flags.contains(vk::MemoryHeapFlags::DEVICE_LOCAL) && video.map_or(true, |v| heap.size >= heaps[v].size) {
                video = Some(i);
            }
        }
        let visible = vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT;
        let rebar = video.is_some_and(|v| types.iter().any(|t| t.heap_index as usize == v && t.property_flags.contains(visible)));
        let mut pools = Vec::new();
        pools.resize_with(types.len() * 2, Pool::default);
        Memory { dedicated: std::array::from_fn(|_| std::sync::atomic::AtomicU64::new(0)), props, rebar, inner: Mutex::new(Inner { pools }) }
    }

    /// What is held per heap (blocks and allocations of their own), in MiB: "heap: MiB (blocks)" — a dev line.
    pub fn usage(&self) -> String {
        let inner = self.inner.lock().unwrap();
        let heaps = self.props.memory_heap_count as usize;
        let mut bytes = vec![0u64; heaps];
        let mut blocks = vec![0usize; heaps];
        for (k, pool) in inner.pools.iter().enumerate() {
            let heap = self.props.memory_types[k / 2].heap_index as usize;
            for b in pool.blocks.iter().flatten() {
                bytes[heap] += b.ranges.size;
                blocks[heap] += 1;
            }
        }
        for t in 0..self.props.memory_type_count as usize {
            bytes[self.props.memory_types[t].heap_index as usize] += self.dedicated[t].load(std::sync::atomic::Ordering::Relaxed);
        }
        let big = format!("; allocations over 1 MiB since last: {} ({} MiB)", BIG.swap(0, std::sync::atomic::Ordering::Relaxed), BIG_BYTES.swap(0, std::sync::atomic::Ordering::Relaxed) >> 20);
        (0..heaps).map(|h| format!("heap {h}: {} MiB ({} blocks)", bytes[h] >> 20, blocks[h])).collect::<Vec<_>>().join(", ") + &big
    }

    pub fn heap_of(&self, type_index: u32) -> u32 {
        self.props.memory_types[type_index as usize].heap_index
    }

    fn heap_size(&self, type_index: u32) -> u64 {
        self.props.memory_heaps[self.props.memory_types[type_index as usize].heap_index as usize].size
    }

    /// The memory types a piece may go to, best first.
    fn candidates(&self, bits: u32, place: Place) -> Vec<u32> {
        let types = &self.props.memory_types[..self.props.memory_type_count as usize];
        let heaps = &self.props.memory_heaps;
        let visible = vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT;
        let mut scored: Vec<(u32, i32)> = Vec::new();
        for (i, t) in types.iter().enumerate() {
            if bits & (1 << i) == 0 {
                continue;
            }
            let flags = t.property_flags;
            let device_local = flags.contains(vk::MemoryPropertyFlags::DEVICE_LOCAL);
            let host = flags.contains(visible);
            let heap_local = heaps[t.heap_index as usize].flags.contains(vk::MemoryHeapFlags::DEVICE_LOCAL);
            let score = match place {
                Place::Device => match (device_local, host) {
                    (true, false) => 3,
                    (true, true) => 2,
                    _ => 1,
                },
                Place::Upload if host => match (heap_local, flags.contains(vk::MemoryPropertyFlags::HOST_CACHED) != upload_cached()) {
                    (false, false) => 4,
                    (false, true) => 3,
                    (true, _) => 1,
                },
                Place::Shared if host => match (device_local, heap_local) {
                    (true, _) => 3,
                    (false, false) => 2,
                    _ => 1,
                },
                Place::Readback if host => match (heap_local, flags.contains(vk::MemoryPropertyFlags::HOST_CACHED)) {
                    (false, true) => 4,
                    (false, false) => 3,
                    (true, true) => 2,
                    (true, false) => 1,
                },
                _ => continue,
            };
            scored.push((i as u32, score));
        }
        // Stable: among equals the driver's own order, which puts its preferred type first.
        scored.sort_by(|a, b| b.1.cmp(&a.1));
        scored.into_iter().map(|(i, _)| i).collect()
    }

    fn block_size(&self, type_index: u32) -> u64 {
        let heap = self.heap_size(type_index);
        if heap > (1 << 30) {
            BLOCK
        } else {
            (heap / 8).max(1 << 20) & !0xFFF
        }
    }

    pub fn allocate(&self, device: &ash::Device, req: vk::MemoryRequirements, place: Place, linear: bool) -> Result<Allocation, vk::Result> {
        let mut last = vk::Result::ERROR_OUT_OF_DEVICE_MEMORY;
        for (n, type_index) in self.candidates(req.memory_type_bits, place).into_iter().enumerate() {
            match self.allocate_in(device, req, type_index, linear) {
                Ok(mut a) => {
                    a.fallback = n > 0;
                    if req.size > (1 << 20) {
                        BIG.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        BIG_BYTES.fetch_add(req.size, std::sync::atomic::Ordering::Relaxed);
                    }
                    if super::stats::on() && (n > 0 || matches!(place, Place::Shared)) {
                        static SEEN: std::sync::Mutex<Vec<(u32, bool, u64)>> = std::sync::Mutex::new(Vec::new());
                        let key = (type_index, n > 0, req.size >> 20);
                        let mut seen = SEEN.lock().unwrap();
                        if !seen.contains(&key) {
                            seen.push(key);
                            eprintln!("[memory] {:?} {} KiB -> type {} heap {}{}", place, req.size >> 10, type_index, self.props.memory_types[type_index as usize].heap_index, if n > 0 { " FALLBACK" } else { "" });
                        }
                    }
                    return Ok(a);
                }
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    fn allocate_in(&self, device: &ash::Device, req: vk::MemoryRequirements, type_index: u32, linear: bool) -> Result<Allocation, vk::Result> {
        let block_size = self.block_size(type_index);
        // Anything the CPU can see is mapped once, for good: mapping per use costs and the game maps often.
        let host = self.props.memory_types[type_index as usize].property_flags.contains(vk::MemoryPropertyFlags::HOST_VISIBLE);
        let cached = self.props.memory_types[type_index as usize].property_flags.contains(vk::MemoryPropertyFlags::HOST_CACHED);
        if req.size > block_size / 2 {
            let (memory, ptr) = Self::raw(device, req.size, type_index, host)?;
            self.dedicated[type_index as usize].fetch_add(req.size, std::sync::atomic::Ordering::Relaxed);
            return Ok(Allocation { memory, offset: 0, size: req.size, ptr, type_index, linear, block: None, fallback: false, cached });
        }
        let mut inner = self.inner.lock().unwrap();
        let pool = &mut inner.pools[type_index as usize * 2 + linear as usize];
        for (i, slot) in pool.blocks.iter_mut().enumerate() {
            if let Some(block) = slot {
                if let Some(at) = block.ranges.take(req.size, req.alignment) {
                    let ptr = if block.ptr.is_null() { block.ptr } else { unsafe { block.ptr.add(at as usize) } };
                    return Ok(Allocation { memory: block.memory, offset: at, size: req.size, ptr, type_index, linear, block: Some(i), fallback: false, cached });
                }
            }
        }
        let (memory, base) = Self::raw(device, block_size, type_index, host)?;
        let mut ranges = Ranges::new(block_size);
        let at = ranges.take(req.size, req.alignment).expect("a fresh block holds half its size");
        let ptr = if base.is_null() { base } else { unsafe { base.add(at as usize) } };
        let block = Block { memory, ptr: base, ranges };
        let i = match pool.blocks.iter().position(Option::is_none) {
            Some(i) => {
                pool.blocks[i] = Some(block);
                i
            }
            None => {
                pool.blocks.push(Some(block));
                pool.blocks.len() - 1
            }
        };
        Ok(Allocation { memory, offset: at, size: req.size, ptr, type_index, linear, block: Some(i), fallback: false, cached })
    }

    fn raw(device: &ash::Device, size: u64, type_index: u32, host: bool) -> Result<(vk::DeviceMemory, *mut u8), vk::Result> {
        let info = vk::MemoryAllocateInfo::default().allocation_size(size).memory_type_index(type_index);
        let t0 = std::time::Instant::now();
        let memory = unsafe { device.allocate_memory(&info, None)? };
        if super::terrain::stalls() {
            eprintln!("[stall] t={:.3} vkAllocateMemory {:.2} MiB of type {type_index} took {:.1} ms", super::terrain::clock(), size as f64 / 1048576.0, t0.elapsed().as_secs_f64() * 1000.0);
        }
        let ptr = if host {
            match unsafe { device.map_memory(memory, 0, vk::WHOLE_SIZE, vk::MemoryMapFlags::empty()) } {
                Ok(p) => p as *mut u8,
                Err(e) => {
                    unsafe { device.free_memory(memory, None) };
                    return Err(e);
                }
            }
        } else {
            std::ptr::null_mut()
        };
        Ok((memory, ptr))
    }

    /// Gives a piece back. The GPU must be done with it (the destruction queue sees to that).
    pub fn free(&self, device: &ash::Device, a: Allocation) {
        let Some(i) = a.block else {
            self.dedicated[a.type_index as usize].fetch_sub(a.size, std::sync::atomic::Ordering::Relaxed);
            if super::terrain::stalls() {
                eprintln!("[stall] t={:.3} vkFreeMemory {:.2} MiB of type {}", super::terrain::clock(), a.size as f64 / 1048576.0, a.type_index);
            }
            unsafe { device.free_memory(a.memory, None) };
            return;
        };
        let mut inner = self.inner.lock().unwrap();
        let pool = &mut inner.pools[a.type_index as usize * 2 + a.linear as usize];
        let block = pool.blocks[i].as_mut().expect("a piece of a block that is gone");
        debug_assert!(block.memory == a.memory);
        block.ranges.give(a.offset, a.size);
        if block.ranges.is_empty() {
            // One empty block stays per pool so a frame that frees and takes again does not go to the driver.
            let spare = pool.blocks.iter().enumerate().any(|(j, b)| j != i && b.as_ref().is_some_and(|b| b.ranges.is_empty()));
            if spare {
                let block = pool.blocks[i].take().unwrap();
                if super::terrain::stalls() {
                    eprintln!("[stall] t={:.3} vkFreeMemory a block of type {}", super::terrain::clock(), a.type_index);
                }
                unsafe { device.free_memory(block.memory, None) };
            }
        }
    }

    pub fn destroy(&self, device: &ash::Device) {
        let mut inner = self.inner.lock().unwrap();
        for pool in &mut inner.pools {
            for block in pool.blocks.drain(..).flatten() {
                unsafe { device.free_memory(block.memory, None) };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Ranges;

    #[test]
    fn takes_aligned_and_gives_back_whole() {
        let mut r = Ranges::new(1024);
        let a = r.take(100, 1).unwrap();
        let b = r.take(100, 256).unwrap();
        let c = r.take(10, 64).unwrap();
        assert_eq!(a, 0);
        assert_eq!(b, 256);
        assert_eq!(c, 128);
        r.give(b, 100);
        r.give(a, 100);
        r.give(c, 10);
        assert!(r.is_empty());
    }

    #[test]
    fn full_block_refuses() {
        let mut r = Ranges::new(256);
        assert_eq!(r.take(256, 256), Some(0));
        assert_eq!(r.take(1, 1), None);
        r.give(0, 256);
        assert!(r.is_empty());
    }

    #[test]
    fn random_churn_never_overlaps() {
        // A small deterministic generator: the same run every time.
        let mut seed = 0x2545F4914F6CDD1Du64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let mut r = Ranges::new(1 << 20);
        let mut live: Vec<(u64, u64)> = Vec::new();
        for _ in 0..20000 {
            if live.is_empty() || next() % 3 != 0 {
                let size = 1 + next() % 5000;
                let align = 1 << (next() % 9);
                if let Some(at) = r.take(size, align) {
                    assert_eq!(at % align, 0);
                    assert!(at + size <= 1 << 20);
                    for &(o, s) in &live {
                        assert!(at + size <= o || o + s <= at, "overlap");
                    }
                    live.push((at, size));
                }
            } else {
                let i = (next() as usize) % live.len();
                let (o, s) = live.swap_remove(i);
                r.give(o, s);
            }
        }
        for (o, s) in live.drain(..) {
            r.give(o, s);
        }
        assert!(r.is_empty());
    }
}

/// CURSED_DEV_UPLOAD_CACHED: upload memory from the host-cached type instead of write-combined, for comparing.
fn upload_cached() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("CURSED_DEV_UPLOAD_CACHED").is_some())
}
