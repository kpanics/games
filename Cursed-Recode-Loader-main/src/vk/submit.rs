//! The submit thread: the frame's command buffers go to the queue from a thread of their own, so the frame thread
//! records the next frame meanwhile. A vkQueueSubmit2 took NVIDIA's driver some 40 µs (12.10, 1440p): at four thousand
//! frames a second that was a sixth of the frame thread's time, and with Improved Transparency the frame thread was
//! what held the GPU up (idle 40 µs a frame between submits).
//!
//! The order stays the frame thread's: submits go in the order handed over, and a frame's present is handed over after
//! its submit and passed on to the present thread (surface.rs) only once that submit is in the queue (the present
//! waits on a semaphore the submit signals). The game waits on submits by the timeline semaphore, which a host may wait
//! on before the signal is submitted. Whatever waits for the queue to be idle (a swapchain going, the end) first waits
//! here until everything handed over is submitted (`flush`). CURSED_DEV_SUBMITINLINE: submits on the frame thread.

use super::device::Gpu;
use super::surface::Job;
use super::Shared;
use ash::vk;
use std::sync::{mpsc, Arc, Condvar, Mutex};

/// One semaphore operation of a submit: semaphore, value, stage.
pub type SemOp = (vk::Semaphore, u64, vk::PipelineStageFlags2);

/// A submit's stages (one VkSubmitInfo2 each): waits, command buffers, signals.
pub struct Batch(pub Vec<(Vec<SemOp>, Vec<vk::CommandBuffer>, Vec<SemOp>)>);

// Command buffers are handles the frame thread is done with once handed over.
unsafe impl Send for Batch {}

enum Msg {
    Submit(Batch),
    Present(Job),
}

pub struct Submitter {
    sender: Mutex<Option<mpsc::Sender<Msg>>>,
    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
    pending: Mutex<u32>,
    idle: Condvar,
}

/// Dev: CURSED_DEV_SUBMITINLINE submits on the frame thread, as before.
pub fn inline() -> bool {
    static INLINE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *INLINE.get_or_init(|| std::env::var_os("CURSED_DEV_SUBMITINLINE").is_some())
}

impl Submitter {
    pub fn new() -> Submitter {
        Submitter { sender: Mutex::new(None), thread: Mutex::new(None), pending: Mutex::new(0), idle: Condvar::new() }
    }

    /// The thread, made as the renderer starts (it keeps the renderer's shared state until `stop`).
    pub fn start(&self, shared: &Arc<Shared>) {
        if inline() {
            return;
        }
        let (tx, rx) = mpsc::channel::<Msg>();
        let owner = shared.clone();
        let handle = std::thread::Builder::new()
            .name("Cursed submit".into())
            .spawn(move || {
                // as the frame thread: the GPU waits for what comes through here
                if std::env::var_os("CURSED_DEV_FRAMENORMAL").is_none() {
                    unsafe {
                        use windows_sys::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_HIGHEST};
                        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST);
                    }
                }
                let gpu = &owner.gpu;
                for msg in rx {
                    match msg {
                        Msg::Submit(b) => {
                            if let Err(e) = submit_now(gpu, &b) {
                                *owner.failure.lock().unwrap() = Some(format!("Failed to submit to the queue: {e:?}"));
                            }
                        }
                        Msg::Present(job) => gpu.presenter.forward(&owner, job),
                    }
                    let mut p = gpu.submitter.pending.lock().unwrap();
                    *p -= 1;
                    gpu.submitter.idle.notify_all();
                }
            })
            .expect("submit thread");
        *self.thread.lock().unwrap() = Some(handle);
        *self.sender.lock().unwrap() = Some(tx);
    }

    fn send(&self, msg: Msg) -> Result<(), Msg> {
        let sender = self.sender.lock().unwrap();
        let Some(s) = sender.as_ref() else { return Err(msg) };
        *self.pending.lock().unwrap() += 1;
        s.send(msg).map_err(|e| {
            *self.pending.lock().unwrap() -= 1;
            e.0
        })
    }

    /// A frame's stages to the queue, in turn.
    pub fn submit(&self, gpu: &Gpu, batch: Batch) {
        if let Err(Msg::Submit(b)) = self.send(Msg::Submit(batch)) {
            if let Err(e) = submit_now(gpu, &b) {
                panic!("Failed to submit to the queue: {e:?}");
            }
        }
    }

    /// A frame's present, after its submit (the present thread counts it already: Presenter::count).
    pub fn present(&self, shared: &Arc<Shared>, job: Job) {
        if let Err(Msg::Present(job)) = self.send(Msg::Present(job)) {
            shared.gpu.presenter.forward(shared, job);
        }
    }

    /// Until everything handed over is in the queue (or with the present thread).
    pub fn flush(&self) {
        let mut p = self.pending.lock().unwrap();
        while *p > 0 {
            p = self.idle.wait(p).unwrap();
        }
    }

    /// The thread ends once it has submitted what it has.
    pub fn stop(&self) {
        self.sender.lock().unwrap().take();
        if let Some(t) = self.thread.lock().unwrap().take() {
            let _ = t.join();
        }
    }
}

pub fn submit_now(gpu: &Gpu, b: &Batch) -> Result<(), vk::Result> {
    let to_info = |op: &SemOp| vk::SemaphoreSubmitInfo::default().semaphore(op.0).value(op.1).stage_mask(op.2);
    let waits: Vec<Vec<vk::SemaphoreSubmitInfo>> = b.0.iter().map(|s| s.0.iter().map(to_info).collect()).collect();
    let signals: Vec<Vec<vk::SemaphoreSubmitInfo>> = b.0.iter().map(|s| s.2.iter().map(to_info).collect()).collect();
    let cmds: Vec<Vec<vk::CommandBufferSubmitInfo>> = b.0.iter().map(|s| s.1.iter().map(|&c| vk::CommandBufferSubmitInfo::default().command_buffer(c)).collect()).collect();
    let submits: Vec<vk::SubmitInfo2> = (0..b.0.len())
        .map(|i| vk::SubmitInfo2::default().wait_semaphore_infos(&waits[i]).command_buffer_infos(&cmds[i]).signal_semaphore_infos(&signals[i]))
        .collect();
    let _queue = gpu.queue_lock.lock().unwrap();
    unsafe { gpu.sync2_d.queue_submit2(gpu.queue, &submits, vk::Fence::null()) }
}
