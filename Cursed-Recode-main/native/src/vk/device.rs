//! The Vulkan instance and device: which GPU, which of its features, which queue.
//!
//! The requirements are the game's own (backend/vulkan/VulkanFeatureSets in 26.3): a GPU that runs Mojang's renderer
//! runs ours and the other way round, so the warnings the game shows about drivers stay true.

use super::memory::Memory;
use ash::vk;
use std::ffi::{c_void, CStr};
use std::sync::Mutex;

/// Why the device could not be made; the order is Java's `BackendCreationException.Reason` (Native.java maps it).
#[derive(Clone, Copy, Debug)]
pub enum Reason {
    Platform = 0,
    LoaderMissing = 1,
    InstanceFailed = 2,
    NoDevice = 3,
    VersionTooLow = 5,
    NoGraphicsQueue = 6,
    MissingExtension = 7,
    MissingFeature = 8,
    Other = 10,
}

#[derive(Debug)]
pub struct Failure {
    pub reason: Reason,
    pub message: String,
    pub missing: Vec<String>,
}

impl Failure {
    fn new(reason: Reason, message: impl Into<String>) -> Self {
        Failure { reason, message: message.into(), missing: Vec::new() }
    }
}

/// What the game asks of the device (DeviceInfo, DeviceLimits, DeviceFeatures).
pub struct Info {
    pub name: String,
    pub vendor: String,
    pub driver: String,
    pub device_type: i32,
    pub timestamp_period: f32,
    pub max_anisotropy: i32,
    pub min_uniform_align: i32,
    pub min_storage_align: u64,
    pub max_texture: i32,
    pub max_allocation: u64,
    pub max_multi_draw: i32,
    pub max_color_attachments: i32,
    pub max_draw_indirect: i32,
    pub wireframe: bool,
    pub multi_draw: bool,
    pub extensions: Vec<String>,
}

pub struct Debug {
    pub instance: ash::ext::debug_utils::Instance,
    pub device: ash::ext::debug_utils::Device,
    messenger: vk::DebugUtilsMessengerEXT,
    /// Object names and command labels: on when the game asks for labels or validation is on.
    pub labels: bool,
}

pub struct Gpu {
    pub entry: ash::Entry,
    pub instance: ash::Instance,
    pub physical: vk::PhysicalDevice,
    pub device: ash::Device,
    pub family: u32,
    pub queue: vk::Queue,
    pub surface_i: ash::khr::surface::Instance,
    pub win32_i: ash::khr::win32_surface::Instance,
    pub swapchain_d: ash::khr::swapchain::Device,
    pub push_d: ash::khr::push_descriptor::Device,
    pub sync2_d: ash::khr::synchronization2::Device,
    pub render_d: ash::khr::dynamic_rendering::Device,
    pub multi_draw_d: Option<ash::ext::multi_draw::Device>,
    /// Memory of Direct3D 11 textures (the window before the game hands its picture to the compositor, twin/present.rs).
    pub external_win32_d: Option<ash::khr::external_memory_win32::Device>,
    /// The adapter's LUID (Direct3D finds the same GPU by it).
    pub luid: Option<[u8; 8]>,
    pub debug: Option<Debug>,
    pub info: Info,
    pub memory: Memory,
    /// The queue is used by the frame thread (submits) and the present thread (presents); Vulkan wants one at a time.
    pub queue_lock: Mutex<()>,
    /// The thread that shows frames (vk/surface.rs).
    pub presenter: super::surface::Presenter,
    /// The submit thread (submit.rs).
    pub submitter: super::submit::Submitter,
}

/// Validation and driver messages, until Java takes them for the game's log.
pub static MESSAGES: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// Whether the device lays shader blocks out as scalars (VK 1.2's scalarBlockLayout, on when it has it).
pub static SCALAR_BLOCKS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

unsafe extern "system" fn on_message(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    _types: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    _user: *mut c_void,
) -> vk::Bool32 {
    let text = if data.is_null() || (*data).p_message.is_null() {
        String::from("<no message>")
    } else {
        CStr::from_ptr((*data).p_message).to_string_lossy().into_owned()
    };
    let level = if severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::ERROR) {
        "E"
    } else if severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::WARNING) {
        "W"
    } else if severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::INFO) {
        "I"
    } else {
        "V"
    };
    if let Ok(mut m) = MESSAGES.lock() {
        // A runaway layer must not eat the memory: past this many unread, the oldest go.
        if m.len() > 4096 {
            m.drain(..1024);
        }
        m.push(format!("{level} {text}"));
    }
    vk::FALSE
}

const REQUIRED_DEVICE: [&CStr; 5] = [
    ash::khr::dynamic_rendering::NAME,
    ash::khr::push_descriptor::NAME,
    ash::khr::synchronization2::NAME,
    ash::ext::vertex_attribute_divisor::NAME,
    ash::khr::swapchain::NAME,
];

fn vendor_name(id: u32) -> String {
    match id {
        0x1002 | 0x1022 => "AMD".into(),
        0x1010 => "IMGTEC".into(),
        0x106B => "APPLE".into(),
        0x10DE | 0x12D2 => "NVIDIA".into(),
        0x13B5 => "ARM".into(),
        0x1414 => "MICROSOFT".into(),
        0x14E4 => "BROADCOM".into(),
        0x168C | 0x17CB | 0x1969 | 0x5143 => "QUALCOMM".into(),
        0x8086 => "INTEL".into(),
        other => format!("0x{other:x}"),
    }
}

fn cstr(chars: &[std::ffi::c_char]) -> String {
    unsafe { CStr::from_ptr(chars.as_ptr()) }.to_string_lossy().into_owned()
}

/// The features the game needs, as a chain the driver fills in or reads.
#[derive(Default)]
struct Features {
    v10: vk::PhysicalDeviceFeatures,
    v11: vk::PhysicalDeviceVulkan11Features<'static>,
    v12: vk::PhysicalDeviceVulkan12Features<'static>,
    sync2: vk::PhysicalDeviceSynchronization2Features<'static>,
    rendering: vk::PhysicalDeviceDynamicRenderingFeatures<'static>,
    divisor: vk::PhysicalDeviceVertexAttributeDivisorFeaturesEXT<'static>,
    multi_draw: vk::PhysicalDeviceMultiDrawFeaturesEXT<'static>,
}

impl Features {
    fn query(instance: &ash::Instance, physical: vk::PhysicalDevice, with_multi_draw: bool) -> Self {
        let mut f = Features::default();
        let mut v2 = vk::PhysicalDeviceFeatures2::default()
            .push_next(&mut f.v11)
            .push_next(&mut f.v12)
            .push_next(&mut f.sync2)
            .push_next(&mut f.rendering)
            .push_next(&mut f.divisor);
        if with_multi_draw {
            v2 = v2.push_next(&mut f.multi_draw);
        }
        unsafe { instance.get_physical_device_features2(physical, &mut v2) };
        f.v10 = v2.features;
        // The chain pointed into `f`; nothing keeps those pointers once the query is done.
        f.v11.p_next = std::ptr::null_mut();
        f.v12.p_next = std::ptr::null_mut();
        f.sync2.p_next = std::ptr::null_mut();
        f.rendering.p_next = std::ptr::null_mut();
        f.divisor.p_next = std::ptr::null_mut();
        f.multi_draw.p_next = std::ptr::null_mut();
        f
    }

    fn missing(&self) -> Vec<String> {
        let mut m = Vec::new();
        let checks: [(&str, u32); 9] = [
            ("multiDrawIndirect", self.v10.multi_draw_indirect),
            ("drawIndirectFirstInstance", self.v10.draw_indirect_first_instance),
            ("samplerAnisotropy", self.v10.sampler_anisotropy),
            ("shaderDrawParameters", self.v11.shader_draw_parameters),
            ("timelineSemaphore", self.v12.timeline_semaphore),
            ("hostQueryReset", self.v12.host_query_reset),
            ("synchronization2", self.sync2.synchronization2),
            ("dynamicRendering", self.rendering.dynamic_rendering),
            ("vertexAttributeInstanceRateDivisor", self.divisor.vertex_attribute_instance_rate_divisor),
        ];
        for (name, on) in checks {
            if on == vk::FALSE {
                m.push(name.to_string());
            }
        }
        m
    }
}

fn device_extensions(instance: &ash::Instance, physical: vk::PhysicalDevice) -> Vec<String> {
    unsafe { instance.enumerate_device_extension_properties(physical) }
        .unwrap_or_default()
        .iter()
        .map(|e| cstr(&e.extension_name))
        .collect()
}

/// The queue family that draws, computes and can show on a Windows window.
fn graphics_family(gpu: &Gpu0, physical: vk::PhysicalDevice) -> Option<u32> {
    let families = unsafe { gpu.instance.get_physical_device_queue_family_properties(physical) };
    families.iter().enumerate().find_map(|(i, f)| {
        let i = i as u32;
        let flags = vk::QueueFlags::GRAPHICS | vk::QueueFlags::COMPUTE;
        let present = unsafe { gpu.win32_i.get_physical_device_win32_presentation_support(physical, i) };
        (f.queue_flags.contains(flags) && present).then_some(i)
    })
}

/// What exists before a device is picked.
struct Gpu0 {
    instance: ash::Instance,
    win32_i: ash::khr::win32_surface::Instance,
}

/// Why this GPU cannot be used, or nothing if it can.
fn unsuitable(gpu: &Gpu0, physical: vk::PhysicalDevice) -> Option<Failure> {
    let props = unsafe { gpu.instance.get_physical_device_properties(physical) };
    let name = cstr(&props.device_name);
    if props.api_version < vk::API_VERSION_1_1 {
        let mut f = Failure::new(Reason::VersionTooLow, format!("{name}: Vulkan 1.1 is missing"));
        f.missing.push("VULKAN_CORE_1_1".into());
        return Some(f);
    }
    let have = device_extensions(&gpu.instance, physical);
    let mut failure: Option<Failure> = None;
    let mut fail = |reason: Reason, what: Vec<String>| {
        let f = failure.get_or_insert_with(|| Failure::new(reason, format!("{name}: missing capabilities")));
        f.reason = reason;
        f.missing.extend(what);
    };
    let features = Features::query(&gpu.instance, physical, false);
    let missing_features = features.missing();
    if !missing_features.is_empty() {
        fail(Reason::MissingFeature, missing_features);
    }
    let missing_ext: Vec<String> =
        REQUIRED_DEVICE.iter().map(|e| e.to_string_lossy().into_owned()).filter(|e| !have.contains(e)).collect();
    if !missing_ext.is_empty() {
        fail(Reason::MissingExtension, missing_ext);
    }
    if graphics_family(gpu, physical).is_none() {
        fail(Reason::NoGraphicsQueue, vec!["COMBINED_GRAPHICS_COMPUTE_PRESENT_QUEUE".into()]);
    }
    if props.api_version < vk::API_VERSION_1_2 {
        fail(Reason::VersionTooLow, vec!["VULKAN_CORE_1_2".into()]);
    }
    failure
}

impl Gpu {
    /// `log_level` is the game's GL/Vulkan debug verbosity (0 = off, up to 4 = everything).
    pub fn new(log_level: i32, labels: bool, validation: bool) -> Result<Gpu, Failure> {
        let entry = unsafe { ash::Entry::load() }
            .map_err(|e| Failure::new(Reason::LoaderMissing, format!("Vulkan loader library is missing: {e}")))?;

        let instance_ext: Vec<String> = unsafe { entry.enumerate_instance_extension_properties(None) }
            .map_err(|e| Failure::new(Reason::InstanceFailed, format!("Error enumerating instance extensions: {e:?}")))?
            .iter()
            .map(|e| cstr(&e.extension_name))
            .collect();
        let layers: Vec<String> = unsafe { entry.enumerate_instance_layer_properties() }
            .unwrap_or_default()
            .iter()
            .map(|l| cstr(&l.layer_name))
            .collect();
        let validation_layer = c"VK_LAYER_KHRONOS_validation";
        let validation = validation && layers.iter().any(|l| l == "VK_LAYER_KHRONOS_validation");
        let mut severity = vk::DebugUtilsMessageSeverityFlagsEXT::empty();
        let levels = [
            vk::DebugUtilsMessageSeverityFlagsEXT::ERROR,
            vk::DebugUtilsMessageSeverityFlagsEXT::WARNING,
            vk::DebugUtilsMessageSeverityFlagsEXT::INFO,
            vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE,
        ];
        for level in levels.iter().take(log_level.clamp(0, 4) as usize) {
            severity |= *level;
        }
        if validation {
            severity |= vk::DebugUtilsMessageSeverityFlagsEXT::ERROR | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING;
        }
        let labels = labels || validation;
        let debug_utils = (!severity.is_empty() || labels) && instance_ext.iter().any(|e| e == "VK_EXT_debug_utils");

        let mut ext_names: Vec<*const std::ffi::c_char> =
            vec![ash::khr::surface::NAME.as_ptr(), ash::khr::win32_surface::NAME.as_ptr()];
        for needed in [ash::khr::surface::NAME, ash::khr::win32_surface::NAME] {
            if !instance_ext.iter().any(|e| e.as_bytes() == needed.to_bytes()) {
                return Err(Failure::new(Reason::Platform, format!("Vulkan has no {}", needed.to_string_lossy())));
            }
        }
        if debug_utils {
            ext_names.push(ash::ext::debug_utils::NAME.as_ptr());
        }
        let layer_names: Vec<*const std::ffi::c_char> = if validation { vec![validation_layer.as_ptr()] } else { vec![] };
        // The same names as Mojang's: drivers keep per-application fixes keyed on them.
        let app = vk::ApplicationInfo::default()
            .application_name(c"Minecraft Java Edition")
            .application_version(0)
            .engine_name(c"MinecraftJE")
            .engine_version(0)
            .api_version(vk::API_VERSION_1_2);
        let mut messenger_info = vk::DebugUtilsMessengerCreateInfoEXT::default()
            .message_severity(severity)
            .message_type(
                vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                    | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                    | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
            )
            .pfn_user_callback(Some(on_message));
        let mut info = vk::InstanceCreateInfo::default()
            .application_info(&app)
            .enabled_extension_names(&ext_names)
            .enabled_layer_names(&layer_names);
        if debug_utils && !severity.is_empty() {
            info = info.push_next(&mut messenger_info);
        }
        let instance = unsafe { entry.create_instance(&info, None) }
            .map_err(|e| Failure::new(Reason::InstanceFailed, format!("Error creating instance: {e:?}")))?;
        let surface_i = ash::khr::surface::Instance::new(&entry, &instance);
        let win32_i = ash::khr::win32_surface::Instance::new(&entry, &instance);

        let messenger = if debug_utils && !severity.is_empty() {
            let utils = ash::ext::debug_utils::Instance::new(&entry, &instance);
            unsafe { utils.create_debug_utils_messenger(&messenger_info, None) }.unwrap_or_default()
        } else {
            vk::DebugUtilsMessengerEXT::null()
        };

        let destroy_instance = |instance: &ash::Instance| unsafe {
            if messenger != vk::DebugUtilsMessengerEXT::null() {
                ash::ext::debug_utils::Instance::new(&entry, instance).destroy_debug_utils_messenger(messenger, None);
            }
            instance.destroy_instance(None);
        };

        let gpu0 = Gpu0 { instance: instance.clone(), win32_i: win32_i.clone() };
        let physicals = match unsafe { instance.enumerate_physical_devices() } {
            Ok(p) if !p.is_empty() => p,
            _ => {
                destroy_instance(&instance);
                return Err(Failure::new(Reason::NoDevice, "No Vulkan capable devices"));
            }
        };
        let mut chosen: Option<vk::PhysicalDevice> = None;
        let mut first_failure: Option<Failure> = None;
        for &p in &physicals {
            match unsuitable(&gpu0, p) {
                Some(f) => {
                    first_failure.get_or_insert(f);
                }
                None => {
                    let discrete = |p: vk::PhysicalDevice| {
                        unsafe { instance.get_physical_device_properties(p) }.device_type == vk::PhysicalDeviceType::DISCRETE_GPU
                    };
                    match chosen {
                        None => chosen = Some(p),
                        Some(c) if discrete(p) && !discrete(c) => {
                            chosen = Some(p);
                            break;
                        }
                        _ => {}
                    }
                }
            }
        }
        let Some(physical) = chosen else {
            destroy_instance(&instance);
            return Err(first_failure.unwrap_or_else(|| Failure::new(Reason::NoDevice, "No Vulkan capable devices")));
        };

        let props = unsafe { instance.get_physical_device_properties(physical) };
        let have = device_extensions(&instance, physical);
        let has = |name: &CStr| have.iter().any(|e| e.as_bytes() == name.to_bytes());
        let with_multi_draw = has(ash::ext::multi_draw::NAME);
        let available = Features::query(&instance, physical, with_multi_draw);
        let multi_draw = with_multi_draw && available.multi_draw.multi_draw == vk::TRUE;
        let wireframe = available.v10.fill_mode_non_solid == vk::TRUE;
        let family = graphics_family(&gpu0, physical).expect("checked by unsuitable()");

        let mut want = Features::default();
        want.v10.multi_draw_indirect = vk::TRUE;
        want.v10.draw_indirect_first_instance = vk::TRUE;
        want.v10.sampler_anisotropy = vk::TRUE;
        want.v10.fill_mode_non_solid = wireframe as u32;
        want.v11.shader_draw_parameters = vk::TRUE;
        want.v12.timeline_semaphore = vk::TRUE;
        want.v12.host_query_reset = vk::TRUE;
        // CS2's own finish compositor lays its constants out so (cosmetics/compose.rs); without it, no finishes
        want.v12.scalar_block_layout = available.v12.scalar_block_layout;
        SCALAR_BLOCKS.store(available.v12.scalar_block_layout == vk::TRUE, std::sync::atomic::Ordering::Release);
        want.sync2.synchronization2 = vk::TRUE;
        want.rendering.dynamic_rendering = vk::TRUE;
        want.divisor.vertex_attribute_instance_rate_divisor = vk::TRUE;
        want.multi_draw.multi_draw = multi_draw as u32;
        let mut device_ext: Vec<*const std::ffi::c_char> = REQUIRED_DEVICE.iter().map(|e| e.as_ptr()).collect();
        if multi_draw {
            device_ext.push(ash::ext::multi_draw::NAME.as_ptr());
        }
        let external_win32 = has(ash::khr::external_memory_win32::NAME);
        if external_win32 {
            device_ext.push(ash::khr::external_memory_win32::NAME.as_ptr());
        }
        // One queue. Measured 12.10: with a second one of the same family for presents (so a present would not hold up
        // a submit) NVIDIA ran every frame's GPU work 1.4 times as long (730 µs against 510).
        let priorities = [1.0f32];
        let queues = [vk::DeviceQueueCreateInfo::default().queue_family_index(family).queue_priorities(&priorities)];
        let mut v2 = vk::PhysicalDeviceFeatures2::default()
            .features(want.v10)
            .push_next(&mut want.v11)
            .push_next(&mut want.v12)
            .push_next(&mut want.sync2)
            .push_next(&mut want.rendering)
            .push_next(&mut want.divisor);
        if multi_draw {
            v2 = v2.push_next(&mut want.multi_draw);
        }
        let device_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queues)
            .enabled_extension_names(&device_ext)
            .push_next(&mut v2);
        let device = match unsafe { instance.create_device(physical, &device_info, None) } {
            Ok(d) => d,
            Err(e) => {
                destroy_instance(&instance);
                return Err(Failure::new(Reason::NoDevice, format!("Failed to create device: {e:?}")));
            }
        };
        let queue = unsafe { device.get_device_queue(family, 0) };

        let mut v11_props = vk::PhysicalDeviceVulkan11Properties::default();
        let mut driver_props = vk::PhysicalDeviceDriverProperties::default();
        let mut multi_props = vk::PhysicalDeviceMultiDrawPropertiesEXT::default();
        {
            let mut p2 = vk::PhysicalDeviceProperties2::default().push_next(&mut v11_props).push_next(&mut driver_props);
            if multi_draw {
                p2 = p2.push_next(&mut multi_props);
            }
            unsafe { instance.get_physical_device_properties2(physical, &mut p2) };
        }
        let limits = props.limits;
        let api = props.api_version;
        let driver = format!(
            "{}.{}.{} {} {}",
            vk::api_version_major(api),
            vk::api_version_minor(api),
            vk::api_version_patch(api),
            cstr(&driver_props.driver_name),
            cstr(&driver_props.driver_info)
        );
        let mut extensions: Vec<String> = Vec::new();
        for e in [ash::khr::surface::NAME, ash::khr::win32_surface::NAME] {
            extensions.push(format!("{} (I)", e.to_string_lossy()));
        }
        if debug_utils {
            extensions.push(format!("{} (I)", ash::ext::debug_utils::NAME.to_string_lossy()));
        }
        for &e in &device_ext {
            extensions.push(format!("{} (D)", unsafe { CStr::from_ptr(e) }.to_string_lossy()));
        }
        let clamp = |v: u32| if v > i32::MAX as u32 { i32::MAX } else { v as i32 };
        let info = Info {
            name: cstr(&props.device_name),
            vendor: vendor_name(props.vendor_id),
            driver,
            device_type: props.device_type.as_raw(),
            timestamp_period: limits.timestamp_period,
            max_anisotropy: limits.max_sampler_anisotropy as i32,
            min_uniform_align: limits.min_uniform_buffer_offset_alignment as i32,
            min_storage_align: limits.min_storage_buffer_offset_alignment,
            max_texture: clamp(limits.max_image_dimension2_d),
            max_allocation: if (v11_props.max_memory_allocation_size as i64) < 0 { i64::MAX as u64 } else { v11_props.max_memory_allocation_size },
            max_multi_draw: if multi_draw { clamp(multi_props.max_multi_draw_count) } else { 0 },
            max_color_attachments: clamp(limits.max_color_attachments),
            max_draw_indirect: clamp(limits.max_draw_indirect_count),
            wireframe,
            multi_draw,
            extensions,
        };

        let debug = debug_utils.then(|| Debug {
            instance: ash::ext::debug_utils::Instance::new(&entry, &instance),
            device: ash::ext::debug_utils::Device::new(&instance, &device),
            messenger,
            labels,
        });
        let memory = Memory::new(&instance, physical);
        Ok(Gpu {
            surface_i,
            win32_i,
            swapchain_d: ash::khr::swapchain::Device::new(&instance, &device),
            push_d: ash::khr::push_descriptor::Device::new(&instance, &device),
            sync2_d: ash::khr::synchronization2::Device::new(&instance, &device),
            render_d: ash::khr::dynamic_rendering::Device::new(&instance, &device),
            multi_draw_d: multi_draw.then(|| ash::ext::multi_draw::Device::new(&instance, &device)),
            external_win32_d: external_win32.then(|| ash::khr::external_memory_win32::Device::new(&instance, &device)),
            luid: (v11_props.device_luid_valid == vk::TRUE).then_some(v11_props.device_luid),
            entry,
            instance,
            physical,
            device,
            family,
            queue,
            queue_lock: Mutex::new(()),
            presenter: super::surface::Presenter::new(),
            submitter: super::submit::Submitter::new(),
            debug,
            info,
            memory,
        })
    }

    pub fn labels(&self) -> Option<&ash::ext::debug_utils::Device> {
        self.debug.as_ref().filter(|d| d.labels).map(|d| &d.device)
    }

    /// Names an object for validation messages and RenderDoc; nothing when labels are off.
    pub fn name<H: vk::Handle>(&self, handle: H, name: &str) {
        if let Some(d) = self.labels() {
            let Ok(c) = std::ffi::CString::new(name.replace('\0', " ")) else { return };
            let info = vk::DebugUtilsObjectNameInfoEXT::default().object_handle(handle).object_name(&c);
            unsafe {
                let _ = d.set_debug_utils_object_name(&info);
            }
        }
    }

    pub fn destroy(&self) {
        self.submitter.stop();
        self.presenter.stop();
        let _queue = self.queue_lock.lock().unwrap();
        unsafe {
            let _ = self.device.device_wait_idle();
            self.memory.destroy(&self.device);
            self.device.destroy_device(None);
            if let Some(d) = &self.debug {
                if d.messenger != vk::DebugUtilsMessengerEXT::null() {
                    d.instance.destroy_debug_utils_messenger(d.messenger, None);
                }
            }
            self.instance.destroy_instance(None);
        }
    }
}
