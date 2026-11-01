//! The command encoder: the GPU work of a frame, recorded as it comes and submitted at the end of the frame.
//!
//! The model is Mojang's (backend/vulkan/VulkanCommandEncoder), so the game sees the same timing:
//! - a timeline semaphore counts submits; at most two are on the GPU, the third waits for the first;
//! - two command pools, one per submit in flight, reset once their submit is done;
//! - everything closed during a frame is destroyed two submits later;
//! - every image stays in the GENERAL layout and a full memory barrier follows each operation outside a render pass.
//!   That is slower than tracking each resource, and it is what the game was tested with; tightening it is a later
//!   step, measured against this.
//!
//! Commands go to the frame's command buffer; objects made mid-frame (a texture's first layout, a buffer's first
//! data) go to an "init" command buffer that runs before it.

use super::device::Gpu;
use super::resources::{Buffer, Doomed, Queries, Texture};
use super::transient::{Piece, Transient};
use ash::vk;

/// Submits on the GPU at once. Mojang's backend has two; at the thousands of frames a second this renderer makes the
/// game's thread, the frame thread and the GPU each take about a third of a frame, and with two in flight they waited
/// on one another (12.10: 2620 FPS with three against 2440, the GPU busy all the frame). A frame more between the
/// input and the picture is a quarter of a millisecond there. CURSED_DEV_INFLIGHT=2: as before.
const IN_FLIGHT: u64 = 3;
/// Slots for per-submit things, enough for the dev switch below.
const SLOTS: usize = 4;

/// CURSED_DEV_INFLIGHT=2|3|4: submits in flight, for comparing.
fn in_flight() -> u64 {
    static N: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *N.get_or_init(|| match std::env::var("CURSED_DEV_INFLIGHT").ok().and_then(|v| v.parse().ok()) {
        Some(n @ 2..=4) => n,
        _ => IN_FLIGHT,
    })
}
/// The number of the first submit; Mojang's counting starts at 2 so "two submits ago" is never negative.
pub const FIRST_SUBMIT: u64 = 2;
/// How long a submit may take before the game is told the GPU is lost.
const TIMEOUT_NS: u64 = 5_000_000_000;

static NO_BARRIERS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

pub fn memory_barrier(gpu: &Gpu, cmd: vk::CommandBuffer) {
    super::stats::add(super::stats::BARRIERS, 1_000_000);
    // dev: what the barriers cost (the picture may break)
    if *NO_BARRIERS.get_or_init(|| std::env::var_os("CURSED_DEV_NOBARRIERS").is_some()) {
        return;
    }
    let barrier = [vk::MemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
        .src_access_mask(vk::AccessFlags2::MEMORY_READ | vk::AccessFlags2::MEMORY_WRITE)
        .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
        .dst_access_mask(vk::AccessFlags2::MEMORY_READ | vk::AccessFlags2::MEMORY_WRITE)];
    let dep = vk::DependencyInfo::default().memory_barriers(&barrier);
    unsafe { gpu.sync2_d.cmd_pipeline_barrier2(cmd, &dep) };
}

/// What a render pass waits for when only render passes came since the last full barrier (Pass::begin): their
/// attachments written, before its fragments test, shade, sample and write — its vertices go on meanwhile, where a full
/// barrier would leave the GPU idle until the pass before ends (about 5 µs a pass at 1440p on an RTX 4060, 12.10). A
/// texture the passes since drew into and the pass reads in a vertex shader makes it wait in full (Pass::push_descriptors).
/// With `models`, also the vertices entity models' dispatches wrote (models.rs) for the vertex input, and with
/// `passes` false only those.
pub fn fragment_barrier(gpu: &Gpu, cmd: vk::CommandBuffer, passes: bool, models: bool) {
    super::stats::add(super::stats::BARRIERS, 1_000_000);
    if *NO_BARRIERS.get_or_init(|| std::env::var_os("CURSED_DEV_NOBARRIERS").is_some()) {
        return;
    }
    let vertices = vk::MemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
        .src_access_mask(vk::AccessFlags2::SHADER_STORAGE_WRITE)
        .dst_stage_mask(vk::PipelineStageFlags2::VERTEX_ATTRIBUTE_INPUT)
        .dst_access_mask(vk::AccessFlags2::VERTEX_ATTRIBUTE_READ);
    let attachments = vk::MemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::ALL_GRAPHICS)
        .src_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE | vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE)
        .dst_stage_mask(
            vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS
                | vk::PipelineStageFlags2::FRAGMENT_SHADER
                | vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS
                | vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
        )
        .dst_access_mask(
            vk::AccessFlags2::SHADER_SAMPLED_READ
                | vk::AccessFlags2::COLOR_ATTACHMENT_READ
                | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE
                | vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_READ
                | vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE,
        );
    let both = [attachments, vertices];
    let barriers: &[vk::MemoryBarrier2] = match (passes, models) {
        (true, true) => &both,
        (true, false) => &both[..1],
        _ => &both[1..],
    };
    let dep = vk::DependencyInfo::default().memory_barriers(barriers);
    unsafe { gpu.sync2_d.cmd_pipeline_barrier2(cmd, &dep) };
}

/// The frame's command buffer for work that depends on nothing the GPU does before it (entity models' dispatches:
/// they read what the CPU wrote and write vertices nothing reads yet): what passes and dispatches left to wait for
/// stays waiting for whatever comes next.
pub fn cmd_unordered(enc: &mut Encoder, gpu: &Gpu) -> vk::CommandBuffer {
    match enc.cmd {
        Some(c) => c,
        None => {
            let (p, m) = (std::mem::take(&mut enc.after_pass), std::mem::take(&mut enc.after_models));
            let c = enc.cmd(gpu);
            (enc.after_pass, enc.after_models) = (p, m);
            c
        }
    }
}

fn check<T>(r: Result<T, vk::Result>, what: &str) -> T {
    r.unwrap_or_else(|e| panic!("{what}: {e:?}"))
}

struct Pool {
    pool: vk::CommandPool,
    buffers: Vec<vk::CommandBuffer>,
    next: usize,
}

impl Pool {
    fn new(gpu: &Gpu) -> Pool {
        let info = vk::CommandPoolCreateInfo::default().flags(vk::CommandPoolCreateFlags::TRANSIENT).queue_family_index(gpu.family);
        let pool = check(unsafe { gpu.device.create_command_pool(&info, None) }, "Failed to create VkCommandPool");
        Pool { pool, buffers: Vec::new(), next: 0 }
    }

    fn take(&mut self, gpu: &Gpu) -> vk::CommandBuffer {
        if self.next == self.buffers.len() {
            let info = vk::CommandBufferAllocateInfo::default()
                .command_pool(self.pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(32);
            let more = check(unsafe { gpu.device.allocate_command_buffers(&info) }, "Failed to allocate VkCommandBuffers");
            self.buffers.extend(more);
        }
        self.next += 1;
        self.buffers[self.next - 1]
    }

    fn reset(&mut self, gpu: &Gpu) {
        check(unsafe { gpu.device.reset_command_pool(self.pool, vk::CommandPoolResetFlags::empty()) }, "Failed to reset VkCommandPool");
        self.next = 0;
    }

    fn destroy(&mut self, gpu: &Gpu) {
        unsafe { gpu.device.destroy_command_pool(self.pool, None) };
    }
}

#[derive(Clone, Copy)]
struct SemaphoreOp {
    semaphore: vk::Semaphore,
    value: u64,
    stage: vk::PipelineStageFlags2,
}

#[derive(Default)]
struct Stage {
    waits: Vec<SemaphoreOp>,
    cmds: Vec<vk::CommandBuffer>,
    signals: Vec<SemaphoreOp>,
}

/// A clear not recorded yet (Encoder::clear_*): the next pass that renders into the texture clears it as it begins.
pub struct PendingClear {
    pub texture: *const Texture,
    pub color: Option<[f32; 4]>,
    pub depth: Option<f32>,
}

pub struct Encoder {
    /// Clears waiting for the pass that renders into their texture (see `clear_color`).
    pub pending_clears: Vec<PendingClear>,
    /// Shared with the game's thread (it waits on it for fences), owned by the renderer.
    timeline: vk::Semaphore,
    /// Runs first in each submit: the copies of mapped transient pieces.
    transient_cmd: vk::CommandBuffer,
    /// The submit being recorded; Mojang's counting starts at 2 so "two submits ago" is never negative.
    pub current: u64,
    pub completed: u64,
    pools: [Pool; SLOTS],
    stages: Vec<Stage>,
    doomed: [Vec<Doomed>; SLOTS],
    doomed_at: usize,
    init: Option<vk::CommandBuffer>,
    cmd: Option<vk::CommandBuffer>,
    pub in_pass: bool,
    /// A render pass ended and nothing waited for it yet: the next pass waits with its fragments (fragment_barrier),
    /// anything else (`cmd`) or the submit's end in full.
    pub after_pass: bool,
    /// Entity models' vertices were dispatched and nothing waited for them yet (models.rs): the next pass's vertex
    /// input waits for them (fragment_barrier), anything else (`cmd`) after a full barrier.
    pub after_models: bool,
    /// The textures render passes drew into since the last full barrier: what a vertex shader may not read yet.
    pub unsettled: Vec<*const super::resources::Texture>,
    /// Textures vertex shaders were seen reading (the lightmap): a pass after one drawn into waits in full.
    pub vertex_read: Vec<*const super::resources::Texture>,
    pub transient: Transient,
    /// CURSED_DEV_BENCH: timestamps at the start and the end of each submit, 4 submits around.
    timer: Option<vk::QueryPool>,
    /// The last submit timed and its last timestamp (for the GPU's idle between submits).
    gpu_end: (u64, u64),
    /// CURSED_DEV_BENCH: timestamps around each render pass, by its label.
    passes: Option<PassTimer>,
}

/// GPU time per render pass label (dev): PASS_TIMESTAMPS per submit, 4 submits around.
pub struct PassTimer {
    pool: vk::QueryPool,
    open: [Vec<(String, u32)>; 4],
    totals: std::collections::HashMap<String, (u64, u64)>,
    frames: u64,
    since: std::time::Instant,
    segment_open: bool,
}

/// Timestamps per submit (the atlas's animation alone is some eighty small passes a frame: at 256 the engine's later
/// work went untimed most frames).
const PASS_TIMESTAMPS: u32 = 1024;

impl Encoder {
    /// The timeline semaphore counting submits; its value starts at `FIRST_SUBMIT - 1`.
    pub fn timeline(gpu: &Gpu) -> vk::Semaphore {
        let mut kind = vk::SemaphoreTypeCreateInfo::default().semaphore_type(vk::SemaphoreType::TIMELINE).initial_value(FIRST_SUBMIT - 1);
        let info = vk::SemaphoreCreateInfo::default().push_next(&mut kind);
        check(unsafe { gpu.device.create_semaphore(&info, None) }, "Failed to create submit VkSemaphore")
    }

    pub fn new(gpu: &Gpu, timeline: vk::Semaphore) -> Encoder {
        let mut e = Encoder {
            timeline,
            transient_cmd: vk::CommandBuffer::null(),
            current: FIRST_SUBMIT,
            completed: 0,
            pools: std::array::from_fn(|_| Pool::new(gpu)),
            stages: vec![Stage::default()],
            doomed: Default::default(),
            doomed_at: 0,
            init: None,
            cmd: None,
            in_pass: false,
            after_pass: false,
            after_models: false,
            unsettled: Vec::new(),
            vertex_read: Vec::new(),
            pending_clears: Vec::new(),
            transient: Transient::new(gpu),
            timer: None,
            gpu_end: (0, 0),
            passes: None,
        };
        // dev: CURSED_DEV_NOTIMER measures without the submit's timestamps (as the game runs)
        if super::stats::on() && std::env::var_os("CURSED_DEV_NOTIMER").is_none() {
            let info = vk::QueryPoolCreateInfo::default().query_type(vk::QueryType::TIMESTAMP).query_count(8);
            e.timer = Some(check(unsafe { gpu.device.create_query_pool(&info, None) }, "Failed to create VkQueryPool"));
            let info = vk::QueryPoolCreateInfo::default().query_type(vk::QueryType::TIMESTAMP).query_count(4 * PASS_TIMESTAMPS);
            let pool = check(unsafe { gpu.device.create_query_pool(&info, None) }, "Failed to create VkQueryPool");
            if super::stats::passes() {
                e.passes = Some(PassTimer {
                    pool,
                    open: Default::default(),
                    totals: Default::default(),
                    frames: 0,
                    since: std::time::Instant::now(),
                    segment_open: false,
                });
            }
        }
        e.begin_transient(gpu);
        e
    }

    fn begin_transient(&mut self, gpu: &Gpu) {
        let cmd = self.begin_buffer(gpu);
        if let Some(pool) = self.timer {
            let slot = (self.current % 4) as u32 * 2;
            unsafe {
                gpu.device.cmd_reset_query_pool(cmd, pool, slot, 2);
                gpu.sync2_d.cmd_write_timestamp2(cmd, vk::PipelineStageFlags2::TOP_OF_PIPE, pool, slot);
            }
        }
        self.transient_cmd = cmd;
        self.execute(gpu, cmd);
    }

    /// Records the whole-block copies of mapped transient pieces (this thread's and the game's) into the command
    /// buffer that runs first, and ends it.
    fn end_transient(&mut self, gpu: &Gpu, game_copies: &[(vk::Buffer, vk::Buffer, u64)]) {
        let cmd = self.transient_cmd;
        let own = self.transient.take_copies();
        if !own.is_empty() || !game_copies.is_empty() {
            self.pass_started(gpu, cmd, "Transient copies");
        }
        for &(src, dst, size) in own.iter().chain(game_copies) {
            unsafe { gpu.device.cmd_copy_buffer(cmd, src, dst, &[vk::BufferCopy { src_offset: 0, dst_offset: 0, size }]) };
        }
        if !own.is_empty() || !game_copies.is_empty() {
            memory_barrier(gpu, cmd);
            self.pass_ended(gpu, cmd);
        }
        check(unsafe { gpu.device.end_command_buffer(cmd) }, "Failed to end VkCommandBuffer");
        self.transient_cmd = vk::CommandBuffer::null();
    }

    /// A command buffer from this submit's pool, begun for one use.
    pub fn begin_buffer(&mut self, gpu: &Gpu) -> vk::CommandBuffer {
        let cmd = self.pools[(self.current % in_flight()) as usize].take(gpu);
        let info = vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        check(unsafe { gpu.device.begin_command_buffer(cmd, &info) }, "Failed to begin VkCommandBuffer");
        cmd
    }

    /// The frame's command buffer for the next command.
    pub fn cmd(&mut self, gpu: &Gpu) -> vk::CommandBuffer {
        let c = match self.cmd {
            Some(c) => c,
            None => {
                assert!(!self.in_pass, "Cannot start command buffer while inside RenderPass");
                let c = self.begin_buffer(gpu);
                self.cmd = Some(c);
                c
            }
        };
        if self.after_pass || self.after_models {
            self.settle(gpu, c);
        }
        c
    }

    /// The full barrier a render pass left for whatever came next (`after_pass`).
    fn settle(&mut self, gpu: &Gpu, cmd: vk::CommandBuffer) {
        self.after_pass = false;
        self.after_models = false;
        memory_barrier(gpu, cmd);
        self.unsettled.clear();
    }

    /// A full barrier recorded (Pass::suspend): nothing drawn before it is unsettled any more.
    pub fn settled(&mut self) {
        self.after_pass = false;
        self.after_models = false;
        self.unsettled.clear();
    }

    /// Where objects made mid-frame record their first commands; runs before the frame's command buffer.
    pub fn init_cmd(&mut self, gpu: &Gpu) -> vk::CommandBuffer {
        if let Some(c) = self.init {
            return c;
        }
        let c = self.begin_buffer(gpu);
        self.init = Some(c);
        c
    }

    fn end_buffers(&mut self, gpu: &Gpu) {
        if self.cmd.is_none() && self.init.is_none() {
            return;
        }
        assert!(!self.in_pass, "Cannot end command buffer while inside RenderPass");
        if let Some(c) = self.init.take() {
            memory_barrier(gpu, c);
            check(unsafe { gpu.device.end_command_buffer(c) }, "Failed to end VkCommandBuffer");
            self.push_cmd(c);
        }
        if let Some(c) = self.cmd.take() {
            if self.after_pass || self.after_models {
                self.settle(gpu, c);
            }
            check(unsafe { gpu.device.end_command_buffer(c) }, "Failed to end VkCommandBuffer");
            self.push_cmd(c);
        }
    }

    fn push_cmd(&mut self, c: vk::CommandBuffer) {
        if !self.stages.last().unwrap().signals.is_empty() {
            self.stages.push(Stage::default());
        }
        self.stages.last_mut().unwrap().cmds.push(c);
    }

    pub fn wait(&mut self, gpu: &Gpu, semaphore: vk::Semaphore, value: u64, stage: vk::PipelineStageFlags2) {
        assert!(!self.in_pass, "Cannot add semaphore operation while inside RenderPass");
        self.end_buffers(gpu);
        let top = self.stages.last().unwrap();
        if !top.cmds.is_empty() || !top.signals.is_empty() {
            self.stages.push(Stage::default());
        }
        self.stages.last_mut().unwrap().waits.push(SemaphoreOp { semaphore, value, stage });
    }

    pub fn execute(&mut self, gpu: &Gpu, c: vk::CommandBuffer) {
        assert!(!self.in_pass, "Cannot execute command buffer while inside RenderPass");
        self.end_buffers(gpu);
        self.push_cmd(c);
    }

    pub fn signal(&mut self, gpu: &Gpu, semaphore: vk::Semaphore, value: u64, stage: vk::PipelineStageFlags2) {
        assert!(!self.in_pass, "Cannot add semaphore operation while inside RenderPass");
        self.end_buffers(gpu);
        self.stages.last_mut().unwrap().signals.push(SemaphoreOp { semaphore, value, stage });
    }

    /// The stages recorded go to the queue: through the submit thread (submit.rs), or here (CURSED_DEV_SUBMITINLINE).
    fn flush_stages(&mut self, gpu: &Gpu) {
        let stages = std::mem::replace(&mut self.stages, vec![Stage::default()]);
        let ops = |v: &Vec<SemaphoreOp>| v.iter().map(|o| (o.semaphore, o.value, o.stage)).collect::<Vec<_>>();
        let batch = super::submit::Batch(stages.iter().map(|s| (ops(&s.waits), s.cmds.clone(), ops(&s.signals))).collect());
        if super::submit::inline() {
            check(super::submit::submit_now(gpu, &batch), "Failed to submit to the queue");
        } else {
            gpu.submitter.submit(gpu, batch);
        }
    }

    /// Dev: a timestamp before a render pass begins (outside it).
    pub fn pass_started(&mut self, gpu: &Gpu, cmd: vk::CommandBuffer, label: &str) {
        let slot = (self.current % 4) as usize;
        if let Some(t) = &mut self.passes {
            let n = t.open[slot].len() as u32 * 2;
            if n + 2 > PASS_TIMESTAMPS {
                return;
            }
            let q = slot as u32 * PASS_TIMESTAMPS + n;
            unsafe {
                gpu.device.cmd_reset_query_pool(cmd, t.pool, q, 2);
                gpu.sync2_d.cmd_write_timestamp2(cmd, vk::PipelineStageFlags2::ALL_COMMANDS, t.pool, q);
            }
            t.open[slot].push((label.to_string(), q));
        }
    }

    /// Dev: a timestamp after the render pass ended.
    pub fn pass_ended(&mut self, gpu: &Gpu, cmd: vk::CommandBuffer) {
        let slot = (self.current % 4) as usize;
        if let Some(t) = &mut self.passes {
            if let Some(&(_, q)) = t.open[slot].last() {
                unsafe { gpu.sync2_d.cmd_write_timestamp2(cmd, vk::PipelineStageFlags2::ALL_COMMANDS, t.pool, q + 1) };
            }
        }
    }

    /// Dev: inside a pass, the time from one pipeline to the next is that pipeline's (host-reset queries: no reset
    /// command may go inside a render pass).
    pub fn segment(&mut self, gpu: &Gpu, cmd: vk::CommandBuffer, pass: &str, pipeline: &str) {
        self.segment_end(gpu, cmd);
        // dev: CURSED_DEV_PASSONLY times whole passes only (a timestamp inside a pass stalls the GPU there)
        static PASS_ONLY: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        if *PASS_ONLY.get_or_init(|| std::env::var_os("CURSED_DEV_PASSONLY").is_some()) {
            return;
        }
        let slot = (self.current % 4) as usize;
        if let Some(t) = &mut self.passes {
            let n = t.open[slot].len() as u32 * 2;
            if n + 2 > PASS_TIMESTAMPS {
                return;
            }
            let q = slot as u32 * PASS_TIMESTAMPS + n;
            unsafe {
                gpu.device.reset_query_pool(t.pool, q, 2);
                gpu.sync2_d.cmd_write_timestamp2(cmd, vk::PipelineStageFlags2::ALL_COMMANDS, t.pool, q);
            }
            t.open[slot].push((format!("  {pass}/{pipeline}"), q));
            t.segment_open = true;
        }
    }

    pub fn segment_end(&mut self, gpu: &Gpu, cmd: vk::CommandBuffer) {
        let slot = (self.current % 4) as usize;
        if let Some(t) = &mut self.passes {
            if t.segment_open {
                t.segment_open = false;
                if let Some(&(_, q)) = t.open[slot].last() {
                    unsafe { gpu.sync2_d.cmd_write_timestamp2(cmd, vk::PipelineStageFlags2::ALL_COMMANDS, t.pool, q + 1) };
                }
            }
        }
    }

    /// `slow`: the submit's first timestamp when it took long, to log where in it the time went (CURSED_DEV_STALLS).
    fn passes_done(&mut self, gpu: &Gpu, submit: u64, slow: Option<u64>) {
        let Some(t) = &mut self.passes else { return };
        let slot = (submit % 4) as usize;
        let mut this = Vec::new();
        let period = gpu.info.timestamp_period as f64;
        for (label, q) in std::mem::take(&mut t.open[slot]) {
            let mut v = [0u64; 2];
            if unsafe { gpu.device.get_query_pool_results(t.pool, q, &mut v, vk::QueryResultFlags::TYPE_64) }.is_ok() && v[1] >= v[0] {
                let ns = ((v[1] - v[0]) as f64 * period) as u64;
                if let Some(base) = slow {
                    this.push(format!("{} at {:.1} for {:.1}", label.trim(), v[0].saturating_sub(base) as f64 * period / 1e3, ns as f64 / 1e3));
                }
                let e = t.totals.entry(label).or_default();
                e.0 += ns;
                e.1 += 1;
            }
        }
        if !this.is_empty() && (super::terrain::stalls() || std::env::var_os("CURSED_DEV_TIMELINE").is_some()) {
            eprintln!("[stall] GPU submit {submit}, us from its start: {}", this.join("; "));
        }
        t.frames += 1;
        if t.since.elapsed().as_secs() >= 10 {
            t.since = std::time::Instant::now();
            let mut all: Vec<_> = t.totals.drain().collect();
            all.sort_by(|a, b| b.1 .0.cmp(&a.1 .0));
            if let Ok(mut m) = super::device::MESSAGES.lock() {
                m.push(format!("I [native] memory held: {}", gpu.memory.usage()));
            }
            let mut text = format!("I [native] GPU per pass, us per frame over {} frames:", t.frames);
            for (label, (ns, n)) in all.iter().take(24) {
                text.push_str(&format!(" {label} {:.1} ({:.1}x);", *ns as f64 / t.frames as f64 / 1000.0, *n as f64 / t.frames as f64));
            }
            t.frames = 0;
            if let Ok(mut m) = super::device::MESSAGES.lock() {
                m.push(text);
            }
        }
    }

    pub fn defer(&mut self, d: Doomed) {
        self.doomed[self.doomed_at].push(d);
    }

    /// The end of a frame: everything recorded goes to the GPU, and the encoder waits until at most two submits are
    /// in flight. `game_copies`: the game's mapped transient pieces of this frame. Returns the newest submit known done.
    pub fn submit(&mut self, gpu: &Gpu, game_copies: &[(vk::Buffer, vk::Buffer, u64)], progress: &super::Progress) -> u64 {
        self.end_buffers(gpu);
        // The transient command buffer was put first in this submit when it began; it is ended now that the frame
        // has listed all its copies.
        self.end_transient(gpu, game_copies);
        let doomed = &mut self.doomed[self.doomed_at];
        self.transient.end(gpu, self.current, doomed);
        if let Some(pool) = self.timer {
            let c = self.begin_buffer(gpu);
            unsafe { gpu.sync2_d.cmd_write_timestamp2(c, vk::PipelineStageFlags2::ALL_COMMANDS, pool, (self.current % 4) as u32 * 2 + 1) };
            check(unsafe { gpu.device.end_command_buffer(c) }, "Failed to end VkCommandBuffer");
            self.execute(gpu, c);
        }
        self.signal(gpu, self.timeline, self.current, vk::PipelineStageFlags2::ALL_COMMANDS);
        let t0 = std::time::Instant::now();
        super::stats::time(super::stats::SUBMIT, || self.flush_stages(gpu));
        let t1 = std::time::Instant::now();
        progress.submitted(self.current);
        self.current += 1;
        let previous = self.current.saturating_sub(in_flight());
        if !super::stats::time(super::stats::WAIT, || self.await_submit(gpu, previous, TIMEOUT_NS)) {
            panic!("5s timeout reached when waiting for VK semaphore");
        }
        let t2 = std::time::Instant::now();
        self.pools[(self.current % in_flight()) as usize].reset(gpu);
        self.doomed_at = (self.doomed_at + 1) % in_flight() as usize;
        let doomed_n = self.doomed[self.doomed_at].len();
        for d in std::mem::take(&mut self.doomed[self.doomed_at]) {
            d.destroy(gpu);
        }
        let t3 = std::time::Instant::now();
        self.transient.recycle(gpu, self.completed);
        // CURSED_DEV_STALLS: which part of a slow submit
        static STALLS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        if *STALLS.get_or_init(|| std::env::var_os("CURSED_DEV_STALLS").is_some()) {
            let ms = |a: std::time::Instant, b: std::time::Instant| b.duration_since(a).as_secs_f64() * 1000.0;
            if ms(t0, std::time::Instant::now()) > 8.0 {
                eprintln!("[stall] t={:.3} submit: queue {:.1} ms, waiting for the GPU {:.1} ms, {} objects destroyed {:.1} ms, recycle {:.1} ms",
                    super::terrain::clock(), ms(t0, t1), ms(t1, t2), doomed_n, ms(t2, t3), ms(t3, std::time::Instant::now()));
            }
        }
        let mut slow = None;
        if let Some(pool) = self.timer {
            let mut t = [0u64; 2];
            let r = unsafe { gpu.device.get_query_pool_results(pool, (previous % 4) as u32 * 2, &mut t, vk::QueryResultFlags::TYPE_64) };
            if r.is_ok() && t[1] > t[0] {
                let ns = ((t[1] - t[0]) as f64 * gpu.info.timestamp_period as f64) as u64;
                super::stats::add(super::stats::GPU, ns);
                if self.gpu_end.0 + 1 == previous && t[0] > self.gpu_end.1 {
                    let gap = ((t[0] - self.gpu_end.1) as f64 * gpu.info.timestamp_period as f64) as u64;
                    if gap < 1_000_000 {
                        super::stats::add(super::stats::GPU_GAP, gap);
                    }
                }
                self.gpu_end = (previous, t[1]);
                if ns > 8_000_000 && super::terrain::stalls() {
                    eprintln!("[stall] GPU submit {previous} took {:.1} ms from its first command to its last", ns as f64 / 1e6);
                    slow = Some(t[0]);
                }
                // CURSED_DEV_TIMELINE: one frame in 3000 laid out, every pass from the submit's start
                static TL: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
                if *TL.get_or_init(|| std::env::var_os("CURSED_DEV_TIMELINE").is_some()) && (previous % 3000 == 0 || (previous % 3000 > 0 && previous % 3000 < 12 && std::env::var_os("CURSED_DEV_TIMELINE_RUN").is_some())) {
                    eprintln!("[timeline] submit {previous}: {:.1} us in all", ns as f64 / 1e3);
                    slow = Some(t[0]);
                }
            }
        }
        self.passes_done(gpu, previous, slow);
        progress.completed(self.completed);
        self.begin_transient(gpu);
        super::stats::frame();
        self.completed
    }

    /// Whether submit `index` is done on the GPU, waiting up to `timeout` nanoseconds for it.
    pub fn await_submit(&mut self, gpu: &Gpu, index: u64, timeout: u64) -> bool {
        if self.completed >= index {
            return true;
        }
        if index >= self.current {
            assert!(timeout == 0, "Cannot wait on a fence for the current submit");
            return false;
        }
        let semaphores = [self.timeline];
        let values = [index];
        let info = vk::SemaphoreWaitInfo::default().semaphores(&semaphores).values(&values);
        match unsafe { gpu.device.wait_semaphores(&info, timeout) } {
            Ok(()) => {
                self.completed = index;
                true
            }
            Err(vk::Result::TIMEOUT) => false,
            Err(e) => panic!("Failed to wait for semaphore: {e:?}"),
        }
    }

    /// A texture's first layout: GENERAL, which it keeps for good.
    pub fn init_texture(&mut self, gpu: &Gpu, t: &Texture) {
        let cmd = self.init_cmd(gpu);
        let barrier = [vk::ImageMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::NONE)
            .src_access_mask(vk::AccessFlags2::NONE)
            .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
            .dst_access_mask(vk::AccessFlags2::MEMORY_READ | vk::AccessFlags2::MEMORY_WRITE)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::GENERAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(t.image)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: t.aspect,
                base_mip_level: 0,
                level_count: t.mips,
                base_array_layer: 0,
                layer_count: t.layers,
            })];
        let dep = vk::DependencyInfo::default().image_memory_barriers(&barrier);
        unsafe { gpu.sync2_d.cmd_pipeline_barrier2(cmd, &dep) };
    }

    /// A new buffer's first content (already in the game's staging memory), copied in before anything of this frame
    /// can read it.
    pub fn init_copy(&mut self, gpu: &Gpu, src: &Buffer, src_offset: u64, dst: &Buffer, len: u64) {
        let cmd = self.init_cmd(gpu);
        let region = vk::BufferCopy { src_offset, dst_offset: 0, size: len };
        unsafe { gpu.device.cmd_copy_buffer(cmd, src.raw, dst.raw, &[region]) };
    }

    /// Bytes put in staging memory for a copy.
    fn stage(&mut self, gpu: &Gpu, data: &[u8]) -> Piece {
        let n = data.len() as u64;
        let piece = self.transient.staging(gpu, n, 1, n, 1);
        unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), piece.host, data.len()) };
        piece
    }

    fn color_range(t: &Texture) -> vk::ImageSubresourceRange {
        vk::ImageSubresourceRange { aspect_mask: vk::ImageAspectFlags::COLOR, base_mip_level: 0, level_count: t.mips, base_array_layer: 0, layer_count: 1 }
    }

    fn clear_color_now(&mut self, gpu: &Gpu, t: &Texture, rgba: [f32; 4]) {
        let cmd = self.cmd(gpu);
        let value = vk::ClearColorValue { float32: rgba };
        unsafe { gpu.device.cmd_clear_color_image(cmd, t.image, vk::ImageLayout::GENERAL, &value, &[Self::color_range(t)]) };
    }

    fn clear_depth_now(&mut self, gpu: &Gpu, t: &Texture, depth: f32) {
        let cmd = self.cmd(gpu);
        let value = vk::ClearDepthStencilValue { depth, stencil: 0 };
        let range = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::DEPTH,
            base_mip_level: 0,
            level_count: t.mips,
            base_array_layer: 0,
            layer_count: 1,
        };
        unsafe { gpu.device.cmd_clear_depth_stencil_image(cmd, t.image, vk::ImageLayout::GENERAL, &value, &[range]) };
    }

    pub fn barrier(&mut self, gpu: &Gpu) {
        let cmd = self.cmd(gpu);
        memory_barrier(gpu, cmd);
    }

    /// A clear of a whole texture waits: the game clears its targets and then renders into them, and a clear as a
    /// pass begins (its load operation) costs the GPU next to nothing, where a clear of its own writes every pixel
    /// (measured 12.10 at 1440p: two colour-and-depth and two depth clears a frame, 46 µs). Anything else that
    /// touches textures first records the clears waiting (`flush_clears`). CURSED_DEV_CLEARNOW: cleared at once.
    pub fn clear_color(&mut self, gpu: &Gpu, t: &Texture, rgba: [f32; 4]) {
        if Self::clear_now() {
            self.clear_color_now(gpu, t, rgba);
            self.barrier(gpu);
            return;
        }
        self.pend(t, Some(rgba), None);
    }

    pub fn clear_depth(&mut self, gpu: &Gpu, t: &Texture, depth: f32) {
        if Self::clear_now() {
            self.clear_depth_now(gpu, t, depth);
            self.barrier(gpu);
            return;
        }
        self.pend(t, None, Some(depth));
    }

    pub fn clear_color_depth(&mut self, gpu: &Gpu, color: &Texture, rgba: [f32; 4], depth_t: &Texture, depth: f32) {
        if Self::clear_now() {
            self.clear_color_now(gpu, color, rgba);
            self.clear_depth_now(gpu, depth_t, depth);
            self.barrier(gpu);
            return;
        }
        self.pend(color, Some(rgba), None);
        self.pend(depth_t, None, Some(depth));
    }

    fn clear_now() -> bool {
        static NOW: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *NOW.get_or_init(|| std::env::var_os("CURSED_DEV_CLEARNOW").is_some())
    }

    /// A texture's clear waiting (a later clear of the same texture replaces the earlier).
    fn pend(&mut self, t: &Texture, color: Option<[f32; 4]>, depth: Option<f32>) {
        // Improved Transparency's depth bounds (cleared to -f32::MAX, a 32-bit float target the size of the window):
        // nothing reads them with its bounds left out (cursed.recode.render.CheapOit, pass.rs cheap_oit_skip)
        if color.is_some_and(|c| c[0] == -f32::MAX) && t.format == vk::Format::R32G32B32A32_SFLOAT && cheap_oit() {
            return;
        }
        let at = t as *const Texture;
        match self.pending_clears.iter_mut().find(|p| std::ptr::eq(p.texture, at)) {
            Some(p) => {
                if color.is_some() {
                    p.color = color;
                }
                if depth.is_some() {
                    p.depth = depth;
                }
            }
            None => self.pending_clears.push(PendingClear { texture: at, color, depth }),
        }
    }

    /// The waiting clear of the texture `view` shows, when a pass rendering into all of it can do it as it begins:
    /// the view is the whole texture (one level, one layer) and the pass covers it. Taken off the list.
    pub fn take_clear(&mut self, view: &super::resources::View, area: vk::Rect2D) -> Option<PendingClear> {
        let t = view.texture();
        let whole = t.mips == 1 && t.layers == 1 && view.base_mip == 0 && area.offset.x == 0 && area.offset.y == 0 && area.extent.width >= t.width && area.extent.height >= t.height;
        if !whole {
            return None;
        }
        let at = self.pending_clears.iter().position(|p| std::ptr::eq(p.texture, t as *const Texture))?;
        Some(self.pending_clears.swap_remove(at))
    }

    /// Records the clears waiting for these textures (a pass draws into them but cannot clear them as it begins).
    pub fn flush_clears_of(&mut self, gpu: &Gpu, textures: &[*const Texture]) {
        if !self.pending_clears.iter().any(|p| textures.contains(&p.texture)) {
            return;
        }
        let (now, later): (Vec<PendingClear>, Vec<PendingClear>) = std::mem::take(&mut self.pending_clears).into_iter().partition(|p| textures.contains(&p.texture));
        self.pending_clears = now;
        self.flush_clears(gpu);
        self.pending_clears = later;
    }

    /// Whether a clear waits for this texture.
    pub fn clear_waits(&self, t: *const Texture) -> bool {
        self.pending_clears.iter().any(|p| std::ptr::eq(p.texture, t))
    }

    /// Records the clears still waiting, as they were asked for.
    pub fn flush_clears(&mut self, gpu: &Gpu) {
        if self.pending_clears.is_empty() {
            return;
        }
        for p in std::mem::take(&mut self.pending_clears) {
            let t = unsafe { &*p.texture };
            if let Some(rgba) = p.color {
                self.clear_color_now(gpu, t, rgba);
            }
            if let Some(d) = p.depth {
                self.clear_depth_now(gpu, t, d);
            }
        }
        self.barrier(gpu);
    }

    /// Clears a rectangle of one mip level of a colour and a depth texture (through a render pass, as only a pass can
    /// clear part of an image).
    #[allow(clippy::too_many_arguments)]
    pub fn clear_region(&mut self, gpu: &Gpu, color: &Texture, rgba: [f32; 4], depth_t: &Texture, depth: f32, rect: [i32; 4], mip: u32) {
        let view = |t: &Texture| {
            let info = vk::ImageViewCreateInfo::default()
                .image(t.image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(t.format)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: t.view_aspect(),
                    base_mip_level: mip,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });
            check(unsafe { gpu.device.create_image_view(&info, None) }, "Failed to create VkImageView")
        };
        let cv = view(color);
        let dv = view(depth_t);
        let cmd = self.cmd(gpu);
        let color_att = [vk::RenderingAttachmentInfo::default()
            .image_view(cv)
            .image_layout(vk::ImageLayout::GENERAL)
            .load_op(vk::AttachmentLoadOp::LOAD)
            .store_op(vk::AttachmentStoreOp::STORE)];
        let depth_att = vk::RenderingAttachmentInfo::default()
            .image_view(dv)
            .image_layout(vk::ImageLayout::GENERAL)
            .load_op(vk::AttachmentLoadOp::LOAD)
            .store_op(vk::AttachmentStoreOp::STORE);
        let area = vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent: vk::Extent2D { width: color.width_at(mip), height: color.height_at(mip) } };
        let info = vk::RenderingInfo::default().render_area(area).layer_count(1).color_attachments(&color_att).depth_attachment(&depth_att);
        let clears = [
            vk::ClearAttachment {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                color_attachment: 0,
                clear_value: vk::ClearValue { color: vk::ClearColorValue { float32: rgba } },
            },
            vk::ClearAttachment {
                aspect_mask: vk::ImageAspectFlags::DEPTH,
                color_attachment: 0,
                clear_value: vk::ClearValue { depth_stencil: vk::ClearDepthStencilValue { depth, stencil: 0 } },
            },
        ];
        let rects = [vk::ClearRect {
            rect: vk::Rect2D {
                offset: vk::Offset2D { x: rect[0], y: rect[1] },
                extent: vk::Extent2D { width: rect[2].max(0) as u32, height: rect[3].max(0) as u32 },
            },
            base_array_layer: 0,
            layer_count: 1,
        }];
        unsafe {
            gpu.render_d.cmd_begin_rendering(cmd, &info);
            gpu.device.cmd_clear_attachments(cmd, &clears, &rects);
            gpu.render_d.cmd_end_rendering(cmd);
        }
        memory_barrier(gpu, cmd);
        self.defer(Doomed::View(Box::new(super::resources::View { raw: cv, texture: std::ptr::null_mut(), base_mip: mip })));
        self.defer(Doomed::View(Box::new(super::resources::View { raw: dv, texture: std::ptr::null_mut(), base_mip: mip })));
    }

    pub fn write_buffer(&mut self, gpu: &Gpu, dst: &Buffer, offset: u64, data: &[u8]) {
        let piece = self.stage(gpu, data);
        let cmd = self.cmd(gpu);
        let region = vk::BufferCopy { src_offset: piece.offset, dst_offset: offset, size: data.len() as u64 };
        unsafe { gpu.device.cmd_copy_buffer(cmd, (*piece.buffer).raw, dst.raw, &[region]) };
        memory_barrier(gpu, cmd);
    }

    pub fn copy_buffer(&mut self, gpu: &Gpu, src: &Buffer, src_offset: u64, dst: &Buffer, dst_offset: u64, size: u64) {
        let cmd = self.cmd(gpu);
        let region = vk::BufferCopy { src_offset, dst_offset, size };
        unsafe { gpu.device.cmd_copy_buffer(cmd, src.raw, dst.raw, &[region]) };
        memory_barrier(gpu, cmd);
    }

    /// Many pieces of one buffer into another: one copy command, one barrier.
    pub fn copy_regions(&mut self, gpu: &Gpu, src: &Buffer, dst: &Buffer, regions: &[vk::BufferCopy]) {
        if regions.is_empty() {
            return;
        }
        let cmd = self.cmd(gpu);
        unsafe { gpu.device.cmd_copy_buffer(cmd, src.raw, dst.raw, regions) };
        memory_barrier(gpu, cmd);
    }

    /// `src_rect` is x, y, row length and image height of the source in texels; `block` the bytes of one texel.
    #[allow(clippy::too_many_arguments)]
    pub fn copy_buffer_to_texture(&mut self, gpu: &Gpu, src: &Buffer, src_offset: u64, src_rect: [u32; 4], block: u64, t: &Texture, dst: [i32; 2], size: [u32; 2], mip: u32, layer: u32) {
        let cmd = self.cmd(gpu);
        let start = (src_rect[0] as u64 + src_rect[1] as u64 * src_rect[2] as u64) * block;
        let region = vk::BufferImageCopy {
            buffer_offset: src_offset + start,
            buffer_row_length: src_rect[2],
            buffer_image_height: src_rect[3],
            image_subresource: vk::ImageSubresourceLayers { aspect_mask: vk::ImageAspectFlags::COLOR, mip_level: mip, base_array_layer: layer, layer_count: 1 },
            image_offset: vk::Offset3D { x: dst[0], y: dst[1], z: 0 },
            image_extent: vk::Extent3D { width: size[0], height: size[1], depth: 1 },
        };
        unsafe { gpu.device.cmd_copy_buffer_to_image(cmd, src.raw, t.image, vk::ImageLayout::GENERAL, &[region]) };
        memory_barrier(gpu, cmd);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn copy_texture_to_buffer(&mut self, gpu: &Gpu, t: &Texture, dst: &Buffer, offset: u64, mip: u32, x: i32, y: i32, w: u32, h: u32) {
        let cmd = self.cmd(gpu);
        let region = vk::BufferImageCopy {
            buffer_offset: offset,
            buffer_row_length: w,
            buffer_image_height: h,
            image_subresource: vk::ImageSubresourceLayers { aspect_mask: t.view_aspect(), mip_level: mip, base_array_layer: 0, layer_count: 1 },
            image_offset: vk::Offset3D { x, y, z: 0 },
            image_extent: vk::Extent3D { width: w, height: h, depth: 1 },
        };
        unsafe { gpu.device.cmd_copy_image_to_buffer(cmd, t.image, vk::ImageLayout::GENERAL, dst.raw, &[region]) };
        memory_barrier(gpu, cmd);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn copy_texture_to_texture(&mut self, gpu: &Gpu, src: &Texture, dst: &Texture, mip: u32, dx: i32, dy: i32, sx: i32, sy: i32, w: u32, h: u32) {
        let cmd = self.cmd(gpu);
        let layers = vk::ImageSubresourceLayers { aspect_mask: src.view_aspect(), mip_level: mip, base_array_layer: 0, layer_count: 1 };
        let region = vk::ImageCopy {
            src_subresource: layers,
            src_offset: vk::Offset3D { x: sx, y: sy, z: 0 },
            dst_subresource: layers,
            dst_offset: vk::Offset3D { x: dx, y: dy, z: 0 },
            extent: vk::Extent3D { width: w, height: h, depth: 1 },
        };
        unsafe { gpu.device.cmd_copy_image(cmd, src.image, vk::ImageLayout::GENERAL, dst.image, vk::ImageLayout::GENERAL, &[region]) };
        memory_barrier(gpu, cmd);
    }

    pub fn timestamp(&mut self, gpu: &Gpu, q: &Queries, index: u32) {
        let cmd = self.cmd(gpu);
        write_timestamp(gpu, cmd, q, index);
    }

    /// The GPU's clock now, in its ticks: a one-query submit of its own, waited for.
    pub fn timestamp_now(&mut self, gpu: &Gpu) -> u64 {
        let q = Queries::new(gpu, 1).unwrap_or_else(|e| panic!("Cannot create query pool: {e:?}"));
        let cmd = self.begin_buffer(gpu);
        unsafe { gpu.sync2_d.cmd_write_timestamp2(cmd, vk::PipelineStageFlags2::ALL_COMMANDS, q.raw, 0) };
        check(unsafe { gpu.device.end_command_buffer(cmd) }, "Failed to end VkCommandBuffer");
        let cmds = [vk::CommandBufferSubmitInfo::default().command_buffer(cmd)];
        let submit = [vk::SubmitInfo2::default().command_buffer_infos(&cmds)];
        {
            let _queue = gpu.queue_lock.lock().unwrap();
            check(unsafe { gpu.sync2_d.queue_submit2(gpu.queue, &submit, vk::Fence::null()) }, "Failed to submit to the queue");
        }
        let mut value = [0u64; 1];
        check(
            unsafe { gpu.device.get_query_pool_results(q.raw, 0, &mut value, vk::QueryResultFlags::TYPE_64 | vk::QueryResultFlags::WAIT) },
            "Cannot fetch current timestamp",
        );
        self.defer(Doomed::Queries(Box::new(q)));
        value[0]
    }

    pub fn destroy(&mut self, gpu: &Gpu) {
        // What the frame was recording goes to the GPU so nothing waits on a command buffer never sent.
        self.end_buffers(gpu);
        self.end_transient(gpu, &[]);
        let doomed = &mut self.doomed[self.doomed_at];
        self.transient.end(gpu, self.current, doomed);
        self.flush_stages(gpu);
        gpu.submitter.flush();
        gpu.presenter.wait_idle();
        unsafe {
            let _queue = gpu.queue_lock.lock().unwrap();
            let _ = gpu.device.queue_wait_idle(gpu.queue);
        }
        for list in &mut self.doomed {
            for d in std::mem::take(list) {
                d.destroy(gpu);
            }
        }
        self.transient.destroy(gpu);
        for p in &mut self.pools {
            p.destroy(gpu);
        }
        if let Some(pool) = self.timer.take() {
            unsafe { gpu.device.destroy_query_pool(pool, None) };
        }
        if let Some(t) = self.passes.take() {
            unsafe { gpu.device.destroy_query_pool(t.pool, None) };
        }
    }
}

pub fn write_timestamp(gpu: &Gpu, cmd: vk::CommandBuffer, q: &Queries, index: u32) {
    unsafe {
        gpu.device.reset_query_pool(q.raw, index, 1);
        gpu.sync2_d.cmd_write_timestamp2(cmd, vk::PipelineStageFlags2::ALL_COMMANDS, q.raw, index);
    }
}

/// Improved Transparency without its depth bounds (cursed.recode.render.CheapOit); CURSED_DEV_VANILLA_OIT: the game's.
pub fn cheap_oit() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("CURSED_DEV_VANILLA_OIT").is_none())
}
