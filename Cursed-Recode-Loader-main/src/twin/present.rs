//! The window before the game shown by the compositor itself (DirectComposition): our Vulkan device draws the
//! picture into a texture Direct3D 11 made and shares, Direct3D copies it into a composition swap chain with
//! per-pixel alpha over the desktop, and it is presented at the monitor's own rate. Nothing goes through the CPU —
//! the old way (read back, UpdateLayeredWindow, ~10 MB a frame) held it to 70–90 uneven frames a second and stalled
//! while the game loaded beside it.
//!
//! Made on the adapter the Vulkan device runs on (by its LUID). If anything here fails the window falls back to the
//! old way (twin/mod.rs).

use crate::vk::device::Gpu;
use ash::vk;
use windows::core::Interface;
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND};
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL_11_0};
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::DirectComposition::{DCompositionCreateDevice, IDCompositionDevice, IDCompositionTarget, IDCompositionVisual};
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::*;

pub struct Composed {
    _dcomp: IDCompositionDevice,
    _target: IDCompositionTarget,
    _visual: IDCompositionVisual,
    ctx: ID3D11DeviceContext,
    swap: IDXGISwapChain2,
    waitable: HANDLE,
    /// Two shared textures in turn: our device draws into one while Direct3D shows the other (no waiting for the GPU
    /// inside a frame).
    shared: [ID3D11Texture2D; 2],
    copied: [ID3D11Query; 2],
    copy_pending: [bool; 2],
    /// The monitor the window is on: its refreshes pace the frames (the compositor's own pace is the main
    /// monitor's — with a 144 Hz main monitor and a 120 Hz second one, every fifth frame was thrown away).
    output: Option<IDXGIOutput>,
    /// The shared textures as our device sees them (B8G8R8A8, the window's size).
    pub images: [vk::Image; 2],
    memory: [vk::DeviceMemory; 2],
    pub width: u32,
    pub height: u32,
}

/// What failed, for the log.
fn why<T>(r: windows::core::Result<T>, what: &str) -> Result<T, String> {
    r.map_err(|e| format!("{what}: {e}"))
}

impl Composed {
    /// The compositor's swap chain for `hwnd` (made with WS_EX_NOREDIRECTIONBITMAP), `w` × `h` px.
    pub unsafe fn new(gpu: &Gpu, hwnd: *mut core::ffi::c_void, w: u32, h: u32) -> Result<Composed, String> {
        let ext = gpu.external_win32_d.as_ref().ok_or("no VK_KHR_external_memory_win32")?;
        let luid = gpu.luid.ok_or("no LUID")?;
        // the same GPU for Direct3D
        let factory: IDXGIFactory2 = why(CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0)), "CreateDXGIFactory2")?;
        let mut adapter: Option<IDXGIAdapter1> = None;
        for i in 0.. {
            let Ok(a) = factory.EnumAdapters1(i) else { break };
            let d = why(a.GetDesc1(), "GetDesc1")?;
            let mine = d.AdapterLuid.LowPart.to_le_bytes().iter().chain(d.AdapterLuid.HighPart.to_le_bytes().iter()).copied().eq(luid.iter().copied());
            if mine {
                adapter = Some(a);
                break;
            }
        }
        let adapter = adapter.ok_or("the Vulkan GPU is not a DXGI adapter")?;
        let monitor = windows_sys::Win32::Graphics::Gdi::MonitorFromWindow(hwnd, windows_sys::Win32::Graphics::Gdi::MONITOR_DEFAULTTONEAREST);
        let mut output = None;
        for i in 0.. {
            let Ok(o) = adapter.EnumOutputs(i) else { break };
            if o.GetDesc().is_ok_and(|d| d.Monitor.0 == monitor) {
                output = Some(o);
                break;
            }
        }
        let mut device: Option<ID3D11Device> = None;
        let mut ctx: Option<ID3D11DeviceContext> = None;
        why(
            D3D11CreateDevice(&adapter, D3D_DRIVER_TYPE_UNKNOWN, None, D3D11_CREATE_DEVICE_BGRA_SUPPORT, Some(&[D3D_FEATURE_LEVEL_11_0]), D3D11_SDK_VERSION, Some(&mut device), None, Some(&mut ctx)),
            "D3D11CreateDevice",
        )?;
        let (device, ctx) = (device.ok_or("no D3D11 device")?, ctx.ok_or("no D3D11 context")?);
        // the swap chain the compositor shows, premultiplied alpha, one frame of latency
        let desc = DXGI_SWAP_CHAIN_DESC1 {
            Width: w,
            Height: h,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            Stereo: false.into(),
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: 2,
            Scaling: DXGI_SCALING_STRETCH,
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
            AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
            Flags: DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT.0 as u32,
        };
        let swap1 = why(factory.CreateSwapChainForComposition(&device, &desc, None), "CreateSwapChainForComposition")?;
        let swap: IDXGISwapChain2 = why(swap1.cast(), "IDXGISwapChain2")?;
        why(swap.SetMaximumFrameLatency(1), "SetMaximumFrameLatency")?;
        let waitable = swap.GetFrameLatencyWaitableObject();
        let dxgi: IDXGIDevice = why(device.cast(), "IDXGIDevice")?;
        let dcomp: IDCompositionDevice = why(DCompositionCreateDevice(&dxgi), "DCompositionCreateDevice")?;
        let target = why(dcomp.CreateTargetForHwnd(HWND(hwnd), true), "CreateTargetForHwnd")?;
        let visual = why(dcomp.CreateVisual(), "CreateVisual")?;
        why(visual.SetContent(&swap), "SetContent")?;
        why(target.SetRoot(&visual), "SetRoot")?;
        why(dcomp.Commit(), "Commit")?;
        // the texture both draw into: Vulkan writes, Direct3D copies
        let tdesc = D3D11_TEXTURE2D_DESC {
            Width: w,
            Height: h,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: (D3D11_BIND_SHADER_RESOURCE.0 | D3D11_BIND_RENDER_TARGET.0) as u32,
            CPUAccessFlags: 0,
            MiscFlags: (D3D11_RESOURCE_MISC_SHARED.0 | D3D11_RESOURCE_MISC_SHARED_NTHANDLE.0) as u32,
        };
        let mut made: Vec<(ID3D11Texture2D, vk::Image, vk::DeviceMemory, ID3D11Query)> = Vec::new();
        for _ in 0..2 {
            let mut shared: Option<ID3D11Texture2D> = None;
            why(device.CreateTexture2D(&tdesc, None, Some(&mut shared)), "CreateTexture2D (shared)")?;
            let shared = shared.ok_or("no shared texture")?;
            let res: IDXGIResource1 = why(shared.cast(), "IDXGIResource1")?;
            let handle = why(res.CreateSharedHandle(None, (DXGI_SHARED_RESOURCE_READ | DXGI_SHARED_RESOURCE_WRITE).0, None), "CreateSharedHandle")?;
            let imported = import(gpu, ext, handle, w, h);
            let _ = CloseHandle(handle);
            let (image, memory) = match imported {
                Ok(m) => m,
                Err(e) => {
                    for (_, i, m, _) in made {
                        gpu.device.destroy_image(i, None);
                        gpu.device.free_memory(m, None);
                    }
                    return Err(e);
                }
            };
            let mut copied: Option<ID3D11Query> = None;
            why(device.CreateQuery(&D3D11_QUERY_DESC { Query: D3D11_QUERY_EVENT, MiscFlags: 0 }, Some(&mut copied)), "CreateQuery")?;
            made.push((shared, image, memory, copied.ok_or("no query")?));
        }
        let (a, b) = (made.remove(0), made.remove(0));
        Ok(Composed {
            _dcomp: dcomp,
            _target: target,
            _visual: visual,
            ctx,
            swap,
            waitable,
            shared: [a.0, b.0],
            copied: [a.3, b.3],
            copy_pending: [false; 2],
            output,
            images: [a.1, b.1],
            memory: [a.2, b.2],
            width: w,
            height: h,
        })
    }

    /// Waits for the next refresh of the window's monitor and until the swap chain takes a frame (at most `ms`).
    pub fn wait(&self, ms: u32) {
        unsafe {
            // paced by the monitor's refreshes; the swap chain's own signal (the compositor's pace) only taken
            if let Some(o) = self.output.as_ref().filter(|o| o.WaitForVBlank().is_ok()) {
                let _ = o;
                windows_sys::Win32::System::Threading::WaitForSingleObjectEx(self.waitable.0, 0, 1);
            } else {
                windows_sys::Win32::System::Threading::WaitForSingleObjectEx(self.waitable.0, ms, 1);
            }
        }
    }

    /// Before our device writes shared texture `slot` again: Direct3D's copy out of it is done.
    pub fn idle(&mut self, slot: usize) {
        if !self.copy_pending[slot] {
            return;
        }
        let until = std::time::Instant::now() + std::time::Duration::from_millis(100);
        unsafe {
            // an event query writes TRUE once done (S_FALSE, still going, writes nothing — and reads as Ok here)
            loop {
                let mut done = windows::Win32::Foundation::BOOL(0);
                let r = self.ctx.GetData(&self.copied[slot], Some(&mut done as *mut _ as *mut core::ffi::c_void), 4, 0);
                if r.is_err() || done.as_bool() || std::time::Instant::now() > until {
                    break;
                }
                std::thread::yield_now();
            }
        }
        self.copy_pending[slot] = false;
    }

    /// Our device finished a frame in shared texture `slot`: into the swap chain, presented at the next refresh.
    pub fn present(&mut self, slot: usize) -> bool {
        unsafe {
            let Ok(back) = self.swap.GetBuffer::<ID3D11Texture2D>(0) else { return false };
            self.ctx.CopyResource(&back, &self.shared[slot]);
            self.ctx.End(&self.copied[slot]);
            self.ctx.Flush();
            self.copy_pending[slot] = true;
            self.swap.Present(1, DXGI_PRESENT(0)).is_ok()
        }
    }

    /// The compositor's count now: frames shown, the refresh the last one was shown at.
    pub fn counts(&self) -> Option<(u32, u32)> {
        unsafe {
            let mut s = DXGI_FRAME_STATISTICS::default();
            self.swap.GetFrameStatistics(&mut s).ok().map(|_| (s.PresentCount, s.PresentRefreshCount))
        }
    }

    /// How the presenting went since `from` (counts()): frames shown and the refreshes they spanned (one frame a
    /// refresh is perfect).
    pub fn stats(&self, from: Option<(u32, u32)>) -> String {
        match (from, self.counts()) {
            (Some(a), Some(b)) if b.0 > a.0 => {
                let (frames, refreshes) = (b.0 - a.0, b.1.wrapping_sub(a.1));
                format!("the compositor showed {frames} frames over {refreshes} refreshes ({:.3} refreshes a frame)", refreshes as f64 / frames as f64)
            }
            _ => "no frame statistics".into(),
        }
    }

    pub fn destroy(self, gpu: &Gpu) {
        unsafe {
            self.ctx.ClearState();
            self.ctx.Flush();
            for k in 0..2 {
                gpu.device.destroy_image(self.images[k], None);
                gpu.device.free_memory(self.memory[k], None);
            }
            let _ = CloseHandle(self.waitable);
        }
    }
}

/// The shared texture as a Vulkan image (B8G8R8A8, a copy target), its memory imported from the handle.
unsafe fn import(gpu: &Gpu, ext: &ash::khr::external_memory_win32::Device, handle: HANDLE, w: u32, h: u32) -> Result<(vk::Image, vk::DeviceMemory), String> {
    let kind = vk::ExternalMemoryHandleTypeFlags::D3D11_TEXTURE;
    let mut external = vk::ExternalMemoryImageCreateInfo::default().handle_types(kind);
    let info = vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .format(vk::Format::B8G8R8A8_UNORM)
        .extent(vk::Extent3D { width: w, height: h, depth: 1 })
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::OPTIMAL)
        .usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::TRANSFER_SRC)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .push_next(&mut external);
    let image = gpu.device.create_image(&info, None).map_err(|e| format!("vkCreateImage (shared): {e:?}"))?;
    let req = gpu.device.get_image_memory_requirements(image);
    let mut props = vk::MemoryWin32HandlePropertiesKHR::default();
    let bits = match ext.get_memory_win32_handle_properties(kind, handle.0 as isize, &mut props) {
        Ok(()) => props.memory_type_bits & req.memory_type_bits,
        Err(_) => req.memory_type_bits,
    };
    let mem_props = gpu.instance.get_physical_device_memory_properties(gpu.physical);
    let pick = |want: vk::MemoryPropertyFlags| (0..mem_props.memory_type_count).find(|&i| bits & (1 << i) != 0 && mem_props.memory_types[i as usize].property_flags.contains(want));
    let Some(type_index) = pick(vk::MemoryPropertyFlags::DEVICE_LOCAL).or_else(|| pick(vk::MemoryPropertyFlags::empty())) else {
        gpu.device.destroy_image(image, None);
        return Err("no memory type for the shared texture".into());
    };
    let mut import = vk::ImportMemoryWin32HandleInfoKHR::default().handle_type(kind).handle(handle.0 as isize);
    let mut dedicated = vk::MemoryDedicatedAllocateInfo::default().image(image);
    let alloc = vk::MemoryAllocateInfo::default().allocation_size(req.size).memory_type_index(type_index).push_next(&mut import).push_next(&mut dedicated);
    let memory = match gpu.device.allocate_memory(&alloc, None) {
        Ok(m) => m,
        Err(e) => {
            gpu.device.destroy_image(image, None);
            return Err(format!("vkAllocateMemory (import): {e:?}"));
        }
    };
    if let Err(e) = gpu.device.bind_image_memory(image, memory, 0) {
        gpu.device.destroy_image(image, None);
        gpu.device.free_memory(memory, None);
        return Err(format!("vkBindImageMemory (shared): {e:?}"));
    }
    Ok((image, memory))
}

/// Our side of the shared texture around the copy into it: taken from the outside (its old contents dropped — the
/// whole of it is written), handed back to the outside after.
pub fn copy_into(gpu: &Gpu, cmd: vk::CommandBuffer, src: vk::Image, dst: vk::Image, w: u32, h: u32) {
    let range = vk::ImageSubresourceRange { aspect_mask: vk::ImageAspectFlags::COLOR, base_mip_level: 0, level_count: 1, base_array_layer: 0, layer_count: 1 };
    let acquire = [vk::ImageMemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
        .dst_stage_mask(vk::PipelineStageFlags2::ALL_TRANSFER)
        .dst_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
        .old_layout(vk::ImageLayout::UNDEFINED)
        .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .src_queue_family_index(vk::QUEUE_FAMILY_EXTERNAL)
        .dst_queue_family_index(gpu.family)
        .image(dst)
        .subresource_range(range)];
    unsafe { gpu.sync2_d.cmd_pipeline_barrier2(cmd, &vk::DependencyInfo::default().image_memory_barriers(&acquire)) };
    let layers = vk::ImageSubresourceLayers { aspect_mask: vk::ImageAspectFlags::COLOR, mip_level: 0, base_array_layer: 0, layer_count: 1 };
    let region = vk::ImageCopy { src_subresource: layers, src_offset: vk::Offset3D::default(), dst_subresource: layers, dst_offset: vk::Offset3D::default(), extent: vk::Extent3D { width: w, height: h, depth: 1 } };
    unsafe { gpu.device.cmd_copy_image(cmd, src, vk::ImageLayout::GENERAL, dst, vk::ImageLayout::TRANSFER_DST_OPTIMAL, &[region]) };
    let release = [vk::ImageMemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::ALL_TRANSFER)
        .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
        .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
        .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .new_layout(vk::ImageLayout::GENERAL)
        .src_queue_family_index(gpu.family)
        .dst_queue_family_index(vk::QUEUE_FAMILY_EXTERNAL)
        .image(dst)
        .subresource_range(range)];
    unsafe { gpu.sync2_d.cmd_pipeline_barrier2(cmd, &vk::DependencyInfo::default().image_memory_barriers(&release)) };
}
