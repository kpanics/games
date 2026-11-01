//! The window's surface: the swapchain the game's finished frame is copied into and shown from.
//!
//! The surface is made from the window's HWND (the game's window is SDL's; Java asks SDL for the handle), so nothing
//! of SDL's Vulkan support is involved. The rest follows Mojang's VulkanGpuSurface: three images or more, one acquire
//! semaphore per frame in flight, one present semaphore per image, and the frame blitted in upside down (the game's
//! textures have their first row at the bottom).
//!
//! The frame thread blits; a present thread of its own (Presenter) shows the frame and right after acquires the image
//! for the next frame, so neither the present (some 120 µs in NVIDIA's driver) nor waiting for a free image is on the
//! frame thread. The queue is used under Gpu::queue_lock. The swapchain is never used by both threads at once: before acquiring or
//! remaking it the frame thread waits until the present thread is done. The game's thread only reads the flags. A
//! swapchain found out of date is reported to the game at its next acquire (as Mojang does for present), and the frame
//! in between is simply not shown.
//!
//! With the window's own title bar (src/chrome) the swapchain is the whole client area: the game's frame goes in under
//! the bar (the game knows only that part as its window), the bar is drawn over the top rows each frame.

use super::device::Gpu;
use super::encoder::Encoder;
use super::resources::View;
use ash::vk;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};

/// What the game's thread and the present thread tell each other about a swapchain.
#[derive(Default)]
pub struct Flags {
    /// The swapchain went out of date; the game's next acquire reports it.
    pub failed: AtomicBool,
    pub suboptimal: AtomicBool,
    /// No more acquires until the game remakes the swapchain.
    out_of_date: AtomicBool,
}

impl Flags {
    fn lost(&self) {
        self.suboptimal.store(true, Ordering::Relaxed);
        self.failed.store(true, Ordering::Relaxed);
        self.out_of_date.store(true, Ordering::Relaxed);
    }
}

pub struct Job {
    swapchain: vk::SwapchainKHR,
    index: u32,
    wait: vk::Semaphore,
    flags: Arc<Flags>,
    /// The window, when the frames go at its monitor's pace (pace.rs).
    paced: Option<isize>,
    /// What to acquire the next frame's image with once this one is shown (null: nothing).
    ahead: vk::Semaphore,
}

/// The next frame's image, acquired by the present thread.
struct Ahead {
    semaphore: vk::Semaphore,
    result: Result<(u32, bool), vk::Result>,
}

/// The present thread: frames to show, one after another, how many are still to be shown, and the image it acquired.
pub struct Presenter {
    sender: Mutex<Option<mpsc::Sender<Job>>>,
    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
    pending: Mutex<u32>,
    idle: Condvar,
    ahead: Mutex<Option<Ahead>>,
}

impl Presenter {
    pub fn new() -> Presenter {
        Presenter { sender: Mutex::new(None), thread: Mutex::new(None), pending: Mutex::new(0), idle: Condvar::new(), ahead: Mutex::new(None) }
    }

    /// A frame to show is coming (through the submit thread: submit.rs), so `wait_idle` waits for it.
    pub fn count(&self) {
        *self.pending.lock().unwrap() += 1;
    }

    /// A frame counted already (`count`), to the present thread.
    pub fn forward(&self, shared: &Arc<super::Shared>, job: Job) {
        let mut sender = self.sender.lock().unwrap();
        if sender.is_none() {
            let (tx, rx) = mpsc::channel::<Job>();
            let owner = shared.clone();
            let handle = std::thread::Builder::new()
                .name("Cursed present".into())
                .spawn(move || {
                    // as the frame thread: the next frame waits for this one's image
                    if std::env::var_os("CURSED_DEV_FRAMENORMAL").is_none() {
                        unsafe {
                            use windows_sys::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_HIGHEST};
                            SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST);
                        }
                    }
                    let gpu = &owner.gpu;
                    for job in rx {
                        present_now(gpu, &job);
                        if let Some(hwnd) = job.paced {
                            super::pace::after_present(hwnd);
                        }
                        if job.ahead != vk::Semaphore::null() && !job.flags.out_of_date.load(Ordering::Relaxed) {
                            let result = super::stats::time(super::stats::ACQUIRE, || unsafe {
                                gpu.swapchain_d.acquire_next_image(job.swapchain, 5_000_000_000, job.ahead, vk::Fence::null())
                            });
                            *gpu.presenter.ahead.lock().unwrap() = Some(Ahead { semaphore: job.ahead, result });
                        }
                        let mut p = gpu.presenter.pending.lock().unwrap();
                        *p -= 1;
                        gpu.presenter.idle.notify_all();
                    }
                })
                .expect("present thread");
            *self.thread.lock().unwrap() = Some(handle);
            *sender = Some(tx);
        }
        if sender.as_ref().unwrap().send(job).is_err() {
            *self.pending.lock().unwrap() -= 1;
            self.idle.notify_all();
        }
    }

    /// Until every frame handed over is shown.
    pub fn wait_idle(&self) {
        let mut p = self.pending.lock().unwrap();
        while *p > 0 {
            p = self.idle.wait(p).unwrap();
        }
    }

    /// An image acquired ahead and not taken (the swapchain going): its semaphore is waited on by an empty submit, so
    /// it is free again.
    fn forget_ahead(&self, gpu: &Gpu) {
        let Some(a) = self.ahead.lock().unwrap().take() else { return };
        if a.result.is_err() {
            return;
        }
        let waits = [vk::SemaphoreSubmitInfo::default().semaphore(a.semaphore).stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)];
        let submit = [vk::SubmitInfo2::default().wait_semaphore_infos(&waits)];
        let _queue = gpu.queue_lock.lock().unwrap();
        unsafe {
            let _ = gpu.sync2_d.queue_submit2(gpu.queue, &submit, vk::Fence::null());
            let _ = gpu.device.queue_wait_idle(gpu.queue);
        }
    }

    /// The thread ends once it has shown what it has.
    pub fn stop(&self) {
        self.sender.lock().unwrap().take();
        if let Some(t) = self.thread.lock().unwrap().take() {
            let _ = t.join();
        }
    }
}

fn present_now(gpu: &Gpu, job: &Job) {
    // dev (CURSED_DEV_STALLS): the screen got nothing new for long
    if super::terrain::stalls() {
        static LAST: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
        let now = std::time::Instant::now();
        if let Some(last) = LAST.lock().unwrap().replace(now) {
            let ms = now.duration_since(last).as_secs_f64() * 1000.0;
            if ms > 30.0 {
                eprintln!("[stall] t={:.3} shown: {ms:.0} ms since the frame shown before", super::terrain::clock());
            }
        }
    }
    let waits = [job.wait];
    let swapchains = [job.swapchain];
    let indices = [job.index];
    let info = vk::PresentInfoKHR::default().wait_semaphores(&waits).swapchains(&swapchains).image_indices(&indices);
    let t0 = std::time::Instant::now();
    let result = super::stats::time(super::stats::PRESENT, || {
        let _queue = gpu.queue_lock.lock().unwrap();
        unsafe { gpu.swapchain_d.queue_present(gpu.queue, &info) }
    });
    if super::terrain::stalls() && t0.elapsed().as_secs_f64() > 0.004 {
        eprintln!("[stall] vkQueuePresent took {:.1} ms", t0.elapsed().as_secs_f64() * 1000.0);
    }
    match result {
        Ok(suboptimal) => {
            if suboptimal {
                job.flags.suboptimal.store(true, Ordering::Relaxed);
            }
        }
        Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => job.flags.lost(),
        // Anything else is the device gone: the game hears it as an out-of-date swapchain and recreates, and the next
        // submit reports the real error.
        Err(_) => job.flags.lost(),
    }
}

/// The game's present modes as bits: IMMEDIATE, MAILBOX, FIFO, FIFO_RELAXED.
fn mode_bit(m: vk::PresentModeKHR) -> u32 {
    match m {
        vk::PresentModeKHR::IMMEDIATE => 1,
        vk::PresentModeKHR::MAILBOX => 2,
        vk::PresentModeKHR::FIFO => 4,
        vk::PresentModeKHR::FIFO_RELAXED => 8,
        _ => 0,
    }
}

fn bit_mode(b: i32) -> vk::PresentModeKHR {
    match b {
        0 => vk::PresentModeKHR::IMMEDIATE,
        1 => vk::PresentModeKHR::MAILBOX,
        2 => vk::PresentModeKHR::FIFO,
        _ => vk::PresentModeKHR::FIFO_RELAXED,
    }
}

pub struct Surface {
    surface: vk::SurfaceKHR,
    format: vk::Format,
    swapchain: vk::SwapchainKHR,
    width: u32,
    height: u32,
    images: Vec<vk::Image>,
    /// One per frame, in turn: a frame's semaphore is acquired with again only when that frame is done (the frame
    /// thread waits for the frame before last at each submit, the present thread acquires one frame ahead).
    acquire: [vk::Semaphore; 3],
    acquire_at: usize,
    present: Vec<vk::Semaphore>,
    image: Option<u32>,
    pub flags: Arc<Flags>,
    pub modes: u32,
    hwnd: isize,
    /// The title bar over the swapchain's top rows (0: none) and what draws it (made with the first bar).
    inset: u32,
    chrome: Option<crate::chrome::paint::Painter>,
    ease: crate::chrome::Ease,
    /// The present mode of the swapchain now (the game's ordinal; -1 none yet).
    mode: i32,
    /// When a frame was last shown (at most a thousand a second: `acquire`).
    shown_at: Option<std::time::Instant>,
}

fn check<T>(r: Result<T, vk::Result>, what: &str) -> T {
    r.unwrap_or_else(|e| panic!("{what}: {e:?}"))
}

impl Surface {
    pub fn new(gpu: &Gpu, hwnd: isize) -> Surface {
        let hinstance = unsafe { windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null()) } as isize;
        let info = vk::Win32SurfaceCreateInfoKHR::default().hinstance(hinstance).hwnd(hwnd);
        let surface = check(unsafe { gpu.win32_i.create_win32_surface(&info, None) }, "Failed to create window surface");
        let modes = check(
            unsafe { gpu.surface_i.get_physical_device_surface_present_modes(gpu.physical, surface) },
            "Failed to enumerate surface present modes",
        );
        let formats = check(unsafe { gpu.surface_i.get_physical_device_surface_formats(gpu.physical, surface) }, "Failed to get surface formats");
        let format = formats
            .iter()
            .find(|f| {
                f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR && (f.format == vk::Format::R8G8B8A8_UNORM || f.format == vk::Format::B8G8R8A8_UNORM)
            })
            .map(|f| f.format)
            .expect("Could not find compatible swapchain format");
        let semaphore = || check(unsafe { gpu.device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None) }, "Failed to create VkSemaphore(binary)");
        Surface {
            surface,
            format,
            swapchain: vk::SwapchainKHR::null(),
            width: 0,
            height: 0,
            images: Vec::new(),
            acquire: [semaphore(), semaphore(), semaphore()],
            acquire_at: 0,
            present: Vec::new(),
            image: None,
            flags: Arc::new(Flags::default()),
            modes: modes.iter().map(|&m| mode_bit(m)).fold(0, |a, b| a | b),
            hwnd,
            inset: 0,
            chrome: None,
            ease: crate::chrome::Ease::default(),
            mode: -1,
            shown_at: None,
        }
    }

    fn destroy_swapchain(&mut self, gpu: &Gpu) {
        let old = self.retire_swapchain(gpu);
        if old != vk::SwapchainKHR::null() {
            unsafe { gpu.swapchain_d.destroy_swapchain(old, None) };
        }
    }

    /// Everything of the swapchain gone but the swapchain itself (returned, to make the next one from).
    fn retire_swapchain(&mut self, gpu: &Gpu) -> vk::SwapchainKHR {
        if self.swapchain == vk::SwapchainKHR::null() {
            return vk::SwapchainKHR::null();
        }
        gpu.submitter.flush();
        gpu.presenter.wait_idle();
        gpu.presenter.forget_ahead(gpu);
        unsafe {
            {
                let _queue = gpu.queue_lock.lock().unwrap();
                let _ = gpu.device.queue_wait_idle(gpu.queue);
            }
            for &s in &self.present {
                gpu.device.destroy_semaphore(s, None);
            }
        }
        self.present.clear();
        self.images.clear();
        std::mem::replace(&mut self.swapchain, vk::SwapchainKHR::null())
    }

    /// A swapchain of `width` × `height` with present mode `mode` (the game's ordinal). An error is for the game
    /// (SurfaceException), not a crash.
    pub fn configure(&mut self, gpu: &Gpu, width: u32, height: u32, mode: i32) -> Result<(), String> {
        self.destroy_swapchain(gpu);
        self.mode = mode;
        let result = self.make_swapchain(gpu, width, height, mode);
        if result.is_err() {
            self.flags.suboptimal.store(true, Ordering::Relaxed);
            self.flags.out_of_date.store(true, Ordering::Relaxed);
        }
        result
    }

    fn make_swapchain(&mut self, gpu: &Gpu, width: u32, height: u32, mode: i32) -> Result<(), String> {
        let caps = unsafe { gpu.surface_i.get_physical_device_surface_capabilities(gpu.physical, self.surface) }
            .map_err(|e| format!("{e:?}: Failed to get surface capabilities"))?;
        // the window's own title bar: the swapchain is the whole client area, the game's frame `height` under the bar
        let inset = crate::chrome::inset_of(self.hwnd).max(0) as u32;
        let (width, height) = if inset > 0 && caps.current_extent.width != u32::MAX && caps.current_extent.height > inset {
            (caps.current_extent.width, caps.current_extent.height)
        } else {
            (width, height)
        };
        self.inset = if caps.current_extent.height > inset { inset } else { 0 };
        let (min, max) = (caps.min_image_extent, caps.max_image_extent);
        if width < min.width || max.width < width || height < min.height || max.height < height {
            return Err(format!(
                "Requested swapchain extent ({width} x {height}) not within allowed extent: min({} x {}) max({} x {})",
                min.width, min.height, max.width, max.height
            ));
        }
        // dev: CURSED_DEV_IMAGES=n images, CURSED_DEV_PRESENT=mailbox|immediate|fifo another present mode (to measure)
        let present_mode = match std::env::var("CURSED_DEV_PRESENT").as_deref() {
            Ok("mailbox") => vk::PresentModeKHR::MAILBOX,
            Ok("immediate") => vk::PresentModeKHR::IMMEDIATE,
            Ok("fifo") => vk::PresentModeKHR::FIFO,
            _ => bit_mode(mode),
        };
        let mut count = caps.min_image_count.max(std::env::var("CURSED_DEV_IMAGES").ok().and_then(|v| v.parse().ok()).unwrap_or(3));
        if caps.max_image_count != 0 {
            count = count.min(caps.max_image_count);
        }
        let info = vk::SwapchainCreateInfoKHR::default()
            .surface(self.surface)
            .min_image_count(count)
            .image_format(self.format)
            .image_color_space(vk::ColorSpaceKHR::SRGB_NONLINEAR)
            .image_extent(vk::Extent2D { width, height })
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::TRANSFER_DST)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(caps.current_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(present_mode)
            .clipped(true);
        self.swapchain = unsafe { gpu.swapchain_d.create_swapchain(&info, None) }.map_err(|e| format!("{e:?}: Failed to create Swapchain"))?;
        self.images = unsafe { gpu.swapchain_d.get_swapchain_images(self.swapchain) }.map_err(|e| format!("{e:?}: Failed to get swapchain images"))?;
        if super::terrain::stalls() {
            eprintln!("[stall] swapchain {width}x{height}: {:?}, {} images", bit_mode(mode), self.images.len());
        }
        for _ in 0..self.images.len() {
            let s = unsafe { gpu.device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None) }
                .map_err(|e| format!("{e:?}: Failed to create VkSemaphore(binary)"))?;
            self.present.push(s);
        }
        self.flags.suboptimal.store(false, Ordering::Relaxed);
        self.flags.failed.store(false, Ordering::Relaxed);
        self.flags.out_of_date.store(false, Ordering::Relaxed);
        self.width = width;
        self.height = height;
        Ok(())
    }

    fn lost(&mut self) {
        self.flags.lost();
        self.image = None;
    }

    /// On the frame thread: the image the present thread acquired after the last frame, or one acquired here (the
    /// first frame of a swapchain). Out of date: no image this frame, the game hears of it at its next acquire.
    pub fn acquire(&mut self, gpu: &Gpu) {
        self.image = None;
        // dev: CURSED_DEV_NOPRESENT draws every frame and shows none (what the showing costs)
        static NOPRESENT: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        if *NOPRESENT.get_or_init(|| std::env::var_os("CURSED_DEV_NOPRESENT").is_some()) && self.swapchain != vk::SwapchainKHR::null() && self.mode >= 0 {
            static SHOWN: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            // the first frames are shown (the stand's pictures need the window), then none
            if SHOWN.fetch_add(1, Ordering::Relaxed) > 20000 {
                return;
            }
        }
        // At most a thousand frames a second go to the window; every frame is still drawn (NVIDIA's Fast Sync's way).
        // Showing one costs the GPU some 100 µs (the copy, then waiting for Windows to give an image back: 12.10, 2430
        // FPS showing none against 1870 showing all), while the screen shows 144 or 165 of them; a frame waits for its
        // turn half a millisecond on average. CURSED_DEV_SHOWCAP=n another limit, 0 none.
        static CAP: std::sync::OnceLock<Option<std::time::Duration>> = std::sync::OnceLock::new();
        let cap = *CAP.get_or_init(|| {
            let n = std::env::var("CURSED_DEV_SHOWCAP").ok().and_then(|v| v.parse::<f64>().ok()).unwrap_or(1000.0);
            (n > 0.0).then(|| std::time::Duration::from_secs_f64(1.0 / n))
        });
        if let Some(gap) = cap {
            let now = std::time::Instant::now();
            if self.shown_at.is_some_and(|t| now.duration_since(t) < gap) {
                return;
            }
            self.shown_at = Some(now);
        }
        // The present thread may still be showing the last frame, or acquiring for this one (and the submit thread
        // handing it the frame).
        gpu.presenter.wait_idle();
        if self.flags.out_of_date.load(Ordering::Relaxed) || self.swapchain == vk::SwapchainKHR::null() {
            gpu.presenter.forget_ahead(gpu);
            return;
        }
        let ahead = gpu.presenter.ahead.lock().unwrap().take();
        let result = match ahead {
            Some(a) => {
                debug_assert!(a.semaphore == self.acquire[self.acquire_at]);
                a.result
            }
            None => {
                self.acquire_at = (self.acquire_at + 1) % self.acquire.len();
                let semaphore = self.acquire[self.acquire_at];
                super::stats::time(super::stats::ACQUIRE, || unsafe {
                    gpu.swapchain_d.acquire_next_image(self.swapchain, 5_000_000_000, semaphore, vk::Fence::null())
                })
            }
        };
        match result {
            Ok((index, suboptimal)) => {
                self.image = Some(index);
                if suboptimal {
                    self.flags.suboptimal.store(true, Ordering::Relaxed);
                }
            }
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => self.lost(),
            Err(vk::Result::TIMEOUT) => panic!("GPU timeout attempting to acquire next frame"),
            Err(e) => panic!("Failed to acquire image: {e:?}"),
        }
    }

    /// Copies the game's frame into the acquired image, upside down, as part of the frame's submit.
    pub fn blit(&mut self, gpu: &Gpu, enc: &mut Encoder, view: &View) {
        let Some(index) = self.image else { return };
        let target = self.images[index as usize];
        let cmd = enc.begin_buffer(gpu);
        enc.pass_started(gpu, cmd, "To the window (after the image is free)");
        let range = vk::ImageSubresourceRange { aspect_mask: vk::ImageAspectFlags::COLOR, base_mip_level: 0, level_count: 1, base_array_layer: 0, layer_count: 1 };
        let to_dst = [vk::ImageMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::NONE)
            .src_access_mask(vk::AccessFlags2::NONE)
            .dst_stage_mask(vk::PipelineStageFlags2::TRANSFER)
            .dst_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(target)
            .subresource_range(range)];
        unsafe { gpu.sync2_d.cmd_pipeline_barrier2(cmd, &vk::DependencyInfo::default().image_memory_barriers(&to_dst)) };
        let t = view.texture();
        let top = self.inset as i32;
        let w = self.width.min(t.width_at(view.base_mip)) as i32;
        let h = (self.height as i32 - top).min(t.height_at(view.base_mip) as i32).max(0);
        let layers = vk::ImageSubresourceLayers { aspect_mask: vk::ImageAspectFlags::COLOR, mip_level: 0, base_array_layer: 0, layer_count: 1 };
        let blit = vk::ImageBlit {
            src_subresource: vk::ImageSubresourceLayers { aspect_mask: vk::ImageAspectFlags::COLOR, mip_level: view.base_mip, base_array_layer: 0, layer_count: 1 },
            src_offsets: [vk::Offset3D { x: 0, y: 0, z: 0 }, vk::Offset3D { x: w, y: h, z: 1 }],
            dst_subresource: layers,
            dst_offsets: [vk::Offset3D { x: 0, y: top + h, z: 0 }, vk::Offset3D { x: w, y: top, z: 1 }],
        };
        // dev: what the copy costs (the window shows nothing)
        static NOBLIT: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        if !*NOBLIT.get_or_init(|| std::env::var_os("CURSED_DEV_NOBLIT").is_some()) {
            unsafe {
                gpu.device.cmd_blit_image(cmd, t.image, vk::ImageLayout::GENERAL, target, vk::ImageLayout::TRANSFER_DST_OPTIMAL, &[blit], vk::Filter::NEAREST)
            };
        }
        // dev: CURSED_DEV_STAMP the frames shown, counted, as 16 black and white blocks at the top left (the stand's
        // pictures: a picture of an old frame shows an old count)
        static STAMP: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        if *STAMP.get_or_init(|| std::env::var_os("CURSED_DEV_STAMP").is_some()) {
            static SHOWN: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            let n = SHOWN.fetch_add(1, Ordering::Relaxed);
            let (bw, bh) = (128u32, 16u32);
            let size = (bw * bh * 4) as u64;
            let p = enc.transient.staging(gpu, size, 16, size, 1);
            unsafe {
                let px = std::slice::from_raw_parts_mut(p.host, size as usize);
                for y in 0..bh {
                    for x in 0..bw {
                        let on = (n >> (15 - x / 8)) & 1 != 0;
                        let at = ((y * bw + x) * 4) as usize;
                        px[at..at + 4].copy_from_slice(&if on { [255, 255, 255, 255] } else { [0, 0, 0, 255] });
                    }
                }
                let region = vk::BufferImageCopy {
                    buffer_offset: p.offset,
                    buffer_row_length: 0,
                    buffer_image_height: 0,
                    image_subresource: layers,
                    image_offset: vk::Offset3D { x: 0, y: top + 40, z: 0 },
                    image_extent: vk::Extent3D { width: bw, height: bh, depth: 1 },
                };
                gpu.device.cmd_copy_buffer_to_image(cmd, (*p.buffer).raw, target, vk::ImageLayout::TRANSFER_DST_OPTIMAL, &[region]);
            }
        }
        // the window's title bar over the top rows
        if top > 0 {
            let look = self.ease.next(self.width as i32).clone();
            super::encoder::memory_barrier(gpu, cmd);
            let painter = self.chrome.get_or_insert_with(|| crate::chrome::paint::Painter::new(gpu, enc, cmd));
            let strip = painter.draw_strip(gpu, enc, cmd, &look);
            let bh = (look.height.min(top)) as i32;
            let bar = vk::ImageBlit {
                src_subresource: layers,
                src_offsets: [vk::Offset3D { x: 0, y: 0, z: 0 }, vk::Offset3D { x: self.width as i32, y: bh, z: 1 }],
                dst_subresource: layers,
                dst_offsets: [vk::Offset3D { x: 0, y: 0, z: 0 }, vk::Offset3D { x: self.width as i32, y: bh, z: 1 }],
            };
            unsafe { gpu.device.cmd_blit_image(cmd, strip, vk::ImageLayout::GENERAL, target, vk::ImageLayout::TRANSFER_DST_OPTIMAL, &[bar], vk::Filter::NEAREST) };
        }
        let to_present = [vk::ImageMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::TRANSFER)
            .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
            .dst_access_mask(vk::AccessFlags2::NONE)
            .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(target)
            .subresource_range(range)];
        let after_read = [vk::MemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::TRANSFER)
            .src_access_mask(vk::AccessFlags2::TRANSFER_READ)
            .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
            .dst_access_mask(vk::AccessFlags2::MEMORY_READ | vk::AccessFlags2::MEMORY_WRITE)];
        let dep = vk::DependencyInfo::default().memory_barriers(&after_read).image_memory_barriers(&to_present);
        unsafe { gpu.sync2_d.cmd_pipeline_barrier2(cmd, &dep) };
        enc.pass_ended(gpu, cmd);
        check(unsafe { gpu.device.end_command_buffer(cmd) }, "Failed to end VkCommandBuffer");
        enc.wait(gpu, self.acquire[self.acquire_at], 0, vk::PipelineStageFlags2::ALL_COMMANDS);
        enc.execute(gpu, cmd);
        enc.signal(gpu, self.present[index as usize], 0, vk::PipelineStageFlags2::TRANSFER);
    }

    /// Hands the acquired image to the present thread, with the semaphore to acquire the next frame's image with; the
    /// frame thread goes on with the next frame.
    ///
    /// On 07.10 a present thread (present only) made things worse: the driver held the queue in vkQueuePresentKHR and
    /// the frame thread's submit waited behind it (1150-1400 FPS against 2120 inline). Measured 12.10 with the acquire
    /// ahead: as fast as inline when the GPU is the limit, and the frame thread has 120 µs a frame more for when it is
    /// not. CURSED_DEV_PRESENTINLINE: shown and acquired on the frame thread, as before.
    pub fn present(&mut self, shared: &Arc<super::Shared>) {
        let Some(index) = self.image.take() else { return };
        // the menus' pace (pace.rs); a swap chain the player asked vertical sync of waits by itself
        let paced = (!matches!(bit_mode(self.mode), vk::PresentModeKHR::FIFO | vk::PresentModeKHR::FIFO_RELAXED)).then_some(self.hwnd);
        let mut job = Job { swapchain: self.swapchain, index, wait: self.present[index as usize], flags: self.flags.clone(), paced, ahead: vk::Semaphore::null() };
        static INLINE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        if *INLINE.get_or_init(|| std::env::var_os("CURSED_DEV_PRESENTINLINE").is_some()) {
            present_now(&shared.gpu, &job);
            if let Some(hwnd) = paced {
                super::pace::after_present(hwnd);
            }
            return;
        }
        self.acquire_at = (self.acquire_at + 1) % self.acquire.len();
        job.ahead = self.acquire[self.acquire_at];
        shared.gpu.presenter.count();
        shared.gpu.submitter.present(shared, job);
    }

    pub fn destroy(&mut self, gpu: &Gpu) {
        self.destroy_swapchain(gpu);
        if let Some(p) = self.chrome.take() {
            p.destroy(gpu);
        }
        unsafe {
            for &s in &self.acquire {
                gpu.device.destroy_semaphore(s, None);
            }
            gpu.surface_i.destroy_surface(self.surface, None);
        }
    }
}
