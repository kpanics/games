//! Our Vulkan renderer for Minecraft 26.3: everything the game's renderer interfaces (com.mojang.renderpearl) ask of a
//! backend, done here. Java (cursed.recode.render) only passes the calls through.
//!
//! Two threads share the work of a frame. The game's render thread describes it: Java writes every command — clears,
//! copies, render passes and their draws, the blit to the window, present — into a stream of bytes and hands it over
//! in pieces (frame.rs lists the commands). The frame thread, ours, turns the stream into Vulkan: it records, submits
//! and presents while the game already builds the next frame. What must answer at once stays on the game's thread:
//! making objects, transient memory (the game writes into it right away), waiting for a frame to finish.

pub mod device;
pub mod encoder;
pub mod frame;
pub mod impostor;
pub mod memory;
pub mod models;
pub mod occlusion;
pub mod pace;
pub mod pass;
pub mod pipeline;
pub mod resources;
pub mod sampler;
pub mod shaders;
pub mod stats;
pub mod submit;
pub mod surface;
pub mod terrain;
pub mod transient;

use ash::vk;
use device::Gpu;
use resources::Doomed;
use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use transient::Transient;

/// How far the frames have come: submitted to the GPU by the frame thread, and known done on the GPU.
pub struct Progress {
    submitted: Mutex<u64>,
    /// The same number, readable without the lock.
    submitted_now: AtomicU64,
    changed: Condvar,
    completed: AtomicU64,
}

impl Progress {
    pub(crate) fn new() -> Progress {
        Progress {
            submitted: Mutex::new(encoder::FIRST_SUBMIT - 1),
            submitted_now: AtomicU64::new(encoder::FIRST_SUBMIT - 1),
            changed: Condvar::new(),
            completed: AtomicU64::new(0),
        }
    }

    pub fn submitted(&self, n: u64) {
        let mut s = self.submitted.lock().unwrap();
        *s = n;
        self.submitted_now.store(n, Ordering::Release);
        drop(s);
        self.changed.notify_all();
    }

    pub fn completed(&self, n: u64) {
        self.completed.fetch_max(n, Ordering::AcqRel);
    }

    pub fn completed_now(&self) -> u64 {
        self.completed.load(Ordering::Acquire)
    }

    /// Waits until frame `n` is submitted; false on timeout. `failed` is checked so a dead frame thread does not hang
    /// the game.
    fn wait_submitted(&self, n: u64, timeout: Option<Duration>, failed: &Mutex<Option<String>>) -> bool {
        let deadline = timeout.map(|t| Instant::now() + t);
        let mut s = self.submitted.lock().unwrap();
        while *s < n {
            if failed.lock().unwrap().is_some() {
                return false;
            }
            let wait = match deadline {
                Some(d) => {
                    let now = Instant::now();
                    if now >= d {
                        return false;
                    }
                    (d - now).min(Duration::from_millis(100))
                }
                None => Duration::from_millis(100),
            };
            s = self.changed.wait_timeout(s, wait).unwrap().0;
        }
        true
    }
}

/// What both threads use.
pub struct Shared {
    pub gpu: Gpu,
    /// Terrain meshes from the game's compile threads.
    pub terrain_feed: terrain::Feed,
    /// What other threads closed or let go; the frame thread destroys it two submits later.
    pub pending: Mutex<Vec<Doomed>>,
    pub progress: Progress,
    /// Counts submits on the GPU; the game's fences wait on it.
    pub timeline: vk::Semaphore,
    /// Why the frame thread stopped, if it did.
    pub failure: Mutex<Option<String>>,
}

unsafe impl Send for Shared {}
unsafe impl Sync for Shared {}

impl Shared {
    pub fn fail_if_dead(&self) {
        if let Some(f) = self.failure.lock().unwrap().as_ref() {
            panic!("The frame thread stopped: {f}");
        }
    }

    /// Whether frame `index` is done on the GPU, waiting up to `timeout` (forever when negative).
    pub fn await_frame(&self, index: u64, timeout_ns: i64) -> bool {
        if self.progress.completed_now() >= index {
            return true;
        }
        if timeout_ns == 0 {
            // A poll (the game's buffer pools ask every frame): no lock, no wait — the timeline's counter as it is.
            if self.progress.submitted_now.load(Ordering::Acquire) < index {
                self.fail_if_dead();
                return false;
            }
            return match unsafe { self.gpu.device.get_semaphore_counter_value(self.timeline) } {
                Ok(v) => {
                    self.progress.completed(v);
                    v >= index
                }
                Err(e) => panic!("Failed to read semaphore: {e:?}"),
            };
        }
        let timeout = (timeout_ns >= 0).then(|| Duration::from_nanos(timeout_ns as u64));
        let start = Instant::now();
        if !self.progress.wait_submitted(index, timeout, &self.failure) {
            self.fail_if_dead();
            return false;
        }
        let semaphores = [self.timeline];
        let values = [index];
        let info = vk::SemaphoreWaitInfo::default().semaphores(&semaphores).values(&values);
        // In slices: a submit that never reaches the queue (the submit thread failed: vk/submit.rs) is a failure seen
        // here, not a wait for ever.
        loop {
            let left = match timeout {
                Some(t) => t.saturating_sub(start.elapsed()).as_nanos() as u64,
                None => u64::MAX,
            };
            match unsafe { self.gpu.device.wait_semaphores(&info, left.min(100_000_000)) } {
                Ok(()) => {
                    self.progress.completed(index);
                    return true;
                }
                Err(vk::Result::TIMEOUT) => {
                    self.fail_if_dead();
                    if left <= 100_000_000 {
                        return false;
                    }
                }
                Err(e) => panic!("Failed to wait for semaphore: {e:?}"),
            }
        }
    }
}

/// What the game's render thread owns.
pub struct Producer {
    /// The game's transient memory: pieces it writes at once while it describes a frame.
    pub transient: Transient,
    /// The frame being described; the same number the frame thread counts.
    pub frame: u64,
    /// Answers with more than one number (a transient piece) are left here for Java to read.
    pub reply: [u64; 8],
}

pub struct Renderer {
    pub shared: Arc<Shared>,
    producer: UnsafeCell<Producer>,
    thread: u32,
    sender: Mutex<Option<mpsc::Sender<frame::Message>>>,
    worker: Mutex<Option<std::thread::JoinHandle<()>>>,
}

unsafe impl Sync for Renderer {}
unsafe impl Send for Renderer {}

pub fn thread_id() -> u32 {
    unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() }
}

impl Renderer {
    pub fn new(gpu: Gpu) -> Renderer {
        let timeline = encoder::Encoder::timeline(&gpu);
        let transient = Transient::new(&gpu);
        let shared = Arc::new(Shared {
            gpu,
            terrain_feed: terrain::Feed::new(),
            pending: Mutex::new(Vec::new()),
            progress: Progress::new(),
            timeline,
            failure: Mutex::new(None),
        });
        shared.gpu.submitter.start(&shared);
        let (sender, receiver) = mpsc::channel();
        let for_thread = shared.clone();
        let worker = std::thread::Builder::new()
            .name("Cursed frame".into())
            .spawn(move || frame::Worker::run(for_thread, receiver))
            .expect("the frame thread");
        Renderer {
            shared,
            producer: UnsafeCell::new(Producer { transient, frame: encoder::FIRST_SUBMIT, reply: [0; 8] }),
            thread: thread_id(),
            sender: Mutex::new(Some(sender)),
            worker: Mutex::new(Some(worker)),
        }
    }

    pub fn gpu(&self) -> &Gpu {
        &self.shared.gpu
    }

    pub fn on_render_thread(&self) -> bool {
        thread_id() == self.thread
    }

    /// The game thread's state; only from the thread that made the device, the game's render thread.
    #[allow(clippy::mut_from_ref)]
    pub fn producer(&self) -> &mut Producer {
        assert!(self.on_render_thread(), "Vulkan frame state used off the render thread");
        unsafe { &mut *self.producer.get() }
    }

    pub fn send(&self, m: frame::Message) {
        self.shared.fail_if_dead();
        let s = self.sender.lock().unwrap();
        s.as_ref().expect("the frame thread is gone").send(m).expect("the frame thread is gone");
    }

    /// Runs `f` on the frame thread after everything sent before, and waits for its answer.
    pub fn call<T: Send + 'static>(&self, f: impl FnOnce(&mut frame::Worker) -> T + Send + 'static) -> T {
        let (tx, rx) = mpsc::sync_channel(1);
        self.send(frame::Message::Call(Box::new(move |w| {
            let _ = tx.send(f(w));
        })));
        match rx.recv() {
            Ok(v) => v,
            Err(_) => {
                self.shared.fail_if_dead();
                panic!("The frame thread did not answer");
            }
        }
    }

    /// The end of a frame description: the last piece goes with the submit and the game's mapped copies; the game's
    /// transient memory turns over. Waits while the frame thread is more than one frame behind. Returns the newest
    /// frame known done on the GPU.
    pub fn submit(&self, ops: &[u8]) -> u64 {
        let p = self.producer();
        let gpu = &self.shared.gpu;
        let copies = p.transient.take_copies();
        let mut bytes = Vec::with_capacity(ops.len() + 8 + copies.len() * 24);
        bytes.extend_from_slice(ops);
        bytes.extend_from_slice(&frame::SUBMIT.to_le_bytes());
        bytes.extend_from_slice(&(copies.len() as i32).to_le_bytes());
        for (src, dst, size) in copies {
            bytes.extend_from_slice(&vk::Handle::as_raw(src).to_le_bytes());
            bytes.extend_from_slice(&vk::Handle::as_raw(dst).to_le_bytes());
            bytes.extend_from_slice(&size.to_le_bytes());
        }
        let mut doomed = Vec::new();
        p.transient.end(gpu, p.frame, &mut doomed);
        if !doomed.is_empty() {
            self.shared.pending.lock().unwrap().extend(doomed);
        }
        self.send(frame::Message::Ops(bytes));
        let sent = p.frame;
        p.frame += 1;
        // At most one frame described ahead of the one being recorded.
        if !self.shared.progress.wait_submitted(sent - 1, None, &self.shared.failure) {
            self.shared.fail_if_dead();
        }
        let completed = self.shared.progress.completed_now();
        p.transient.recycle(gpu, completed);
        completed
    }

    /// Destroys `d` once no frame in flight can use it, from any thread.
    pub fn defer(&self, d: Doomed) {
        self.shared.pending.lock().unwrap().push(d);
    }

    pub fn destroy(&self) {
        let (tx, rx) = mpsc::sync_channel(1);
        if self.sender.lock().unwrap().as_ref().map(|s| s.send(frame::Message::Exit(tx))).is_some() {
            let _ = rx.recv();
        }
        self.sender.lock().unwrap().take();
        if let Some(w) = self.worker.lock().unwrap().take() {
            let _ = w.join();
        }
        let gpu = &self.shared.gpu;
        gpu.submitter.stop();
        gpu.presenter.stop();
        unsafe {
            let _queue = gpu.queue_lock.lock().unwrap();
            let _ = gpu.device.device_wait_idle();
        }
        for d in std::mem::take(&mut *self.shared.pending.lock().unwrap()) {
            d.destroy(gpu);
        }
        let p = unsafe { &mut *self.producer.get() };
        p.transient.destroy(gpu);
        unsafe { gpu.device.destroy_semaphore(self.shared.timeline, None) };
        gpu.destroy();
    }
}
