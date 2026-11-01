//! Entity models' vertices made by the GPU (shaders/models.comp).
//!
//! The parts' polygons are fixed once baked (scene/models.rs GPU_MESH) and live in video memory. When the game draws a
//! model into its per-frame vertex buffer, Java leaves the part's vertices unwritten in the game's staging memory and
//! writes one job per drawn part instead — its pose, normal matrix, colour, light, overlay and where its vertices go
//! (cursed.recode.scene.ModelJobs); the upload skips those ranges, and after the game's copy into the vertex buffer this
//! dispatch fills them. 128 bytes per part instead of 36 per vertex: a zombie in armour is ~0.4 KB instead of ~14 KB.
//!
//! CURSED_DEV_GPUMODELCHECK reads every dispatch's vertices back and compares them with scene/models.rs `vertices`
//! (the CPU path, itself checked bit for bit against the game).

use super::device::Gpu;
use super::encoder::Encoder;
use super::memory::Place;
use super::resources::{Buffer, Doomed};
use crate::scene::models::{self, GPU_MESH, MESH_WORDS};
use ash::vk;

pub const JOB_WORDS: usize = 32;
pub const JOB: usize = JOB_WORDS * 4;
const NO_DESTINATION: u32 = u32::MAX;
const GROUPS_X: u32 = 65535;

struct Pending {
    submit: u64,
    readback: Buffer,
    /// Where in the target the read-back range starts, in words.
    from: u32,
    jobs: Vec<u32>,
}

#[derive(Default)]
struct Check {
    pending: Vec<Pending>,
    parts: u64,
    differ: u64,
    printed: u64,
}

pub struct GpuModels {
    pipeline: vk::Pipeline,
    layout: vk::PipelineLayout,
    set_layout: vk::DescriptorSetLayout,
    module: vk::ShaderModule,
    mesh: Option<Buffer>,
    /// Vertices of GPU_MESH already in `mesh`.
    uploaded: usize,
    check: Option<Check>,
}

fn fail<T>(r: Result<T, vk::Result>, what: &str) -> T {
    r.unwrap_or_else(|e| panic!("{what}: {e:?}"))
}

impl GpuModels {
    pub fn new(gpu: &Gpu, spirv: &[u32], check: bool) -> GpuModels {
        let d = &gpu.device;
        let module = fail(unsafe { d.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(spirv), None) }, "Can't make the models shader");
        let bindings: Vec<vk::DescriptorSetLayoutBinding> = (0..3)
            .map(|i| {
                vk::DescriptorSetLayoutBinding::default()
                    .binding(i)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE)
            })
            .collect();
        let set_info = vk::DescriptorSetLayoutCreateInfo::default().flags(vk::DescriptorSetLayoutCreateFlags::PUSH_DESCRIPTOR_KHR).bindings(&bindings);
        let set_layout = fail(unsafe { d.create_descriptor_set_layout(&set_info, None) }, "Can't make the models descriptor layout");
        let ranges = [vk::PushConstantRange { stage_flags: vk::ShaderStageFlags::COMPUTE, offset: 0, size: 4 }];
        let set_layouts = [set_layout];
        let layout_info = vk::PipelineLayoutCreateInfo::default().set_layouts(&set_layouts).push_constant_ranges(&ranges);
        let layout = fail(unsafe { d.create_pipeline_layout(&layout_info, None) }, "Can't make the models pipeline layout");
        let stage = vk::PipelineShaderStageCreateInfo::default().stage(vk::ShaderStageFlags::COMPUTE).module(module).name(c"main");
        let info = [vk::ComputePipelineCreateInfo::default().stage(stage).layout(layout)];
        let pipeline = unsafe { d.create_compute_pipelines(vk::PipelineCache::null(), &info, None) }.map(|v| v[0]).map_err(|(_, e)| e);
        let pipeline = fail(pipeline, "Can't make the models pipeline");
        gpu.name(pipeline, "Pipeline cursed:models");
        GpuModels { pipeline, layout, set_layout, module, mesh: None, uploaded: 0, check: check.then(Check::default) }
    }

    /// New parts' vertices go to video memory; the buffer grows by copying on the GPU.
    fn upload_mesh(&mut self, gpu: &Gpu, enc: &mut Encoder) {
        let mesh = GPU_MESH.read().unwrap();
        let total = mesh.data.len();
        if total == self.uploaded {
            return;
        }
        let vertex = (MESH_WORDS * 4) as u64;
        let need = total as u64 * vertex;
        if self.mesh.as_ref().is_none_or(|b| b.size < need) {
            let size = (need * 2).max(1 << 20);
            let flags = vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST | vk::BufferUsageFlags::TRANSFER_SRC;
            let bigger = Buffer::with(gpu, flags, size, Place::Device).unwrap_or_else(|e| panic!("Failed to allocate the models' mesh: {e:?}"));
            gpu.name(bigger.raw, "Cursed models mesh");
            if let Some(old) = self.mesh.take() {
                if self.uploaded > 0 {
                    enc.copy_buffer(gpu, &old, 0, &bigger, 0, self.uploaded as u64 * vertex);
                }
                enc.defer(Doomed::Buffer(Box::new(old)));
            }
            self.mesh = Some(bigger);
        }
        let tail = &mesh.data[self.uploaded..];
        let bytes = unsafe { std::slice::from_raw_parts(tail.as_ptr() as *const u8, tail.len() * MESH_WORDS * 4) };
        enc.write_buffer(gpu, self.mesh.as_ref().unwrap(), self.uploaded as u64 * vertex, bytes);
        self.uploaded = total;
    }

    /// `count` jobs at `jobs`/`offset` (host memory the GPU reads) fill their parts' vertices in `target`.
    pub fn dispatch(&mut self, gpu: &Gpu, enc: &mut Encoder, jobs: &Buffer, offset: u64, count: u32, target: &Buffer) {
        if count == 0 {
            return;
        }
        self.upload_mesh(gpu, enc);
        let mesh = self.mesh.as_ref().expect("models' mesh");
        // no barrier before: nothing the GPU did before is read or overwritten (Encoder::cmd_unordered); the vertices
        // are waited for by the next pass's vertex input (Encoder::after_models, encoder::fragment_barrier)
        let cmd = super::encoder::cmd_unordered(enc, gpu);
        enc.pass_started(gpu, cmd, "Models (compute)");
        let infos = [
            vk::DescriptorBufferInfo { buffer: mesh.raw, offset: 0, range: vk::WHOLE_SIZE },
            vk::DescriptorBufferInfo { buffer: jobs.raw, offset, range: count as u64 * JOB as u64 },
            vk::DescriptorBufferInfo { buffer: target.raw, offset: 0, range: vk::WHOLE_SIZE },
        ];
        let writes: Vec<vk::WriteDescriptorSet> = (0..3)
            .map(|i| {
                vk::WriteDescriptorSet::default()
                    .dst_binding(i as u32)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(std::slice::from_ref(&infos[i]))
            })
            .collect();
        unsafe {
            gpu.device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.pipeline);
            gpu.push_d.cmd_push_descriptor_set(cmd, vk::PipelineBindPoint::COMPUTE, self.layout, 0, &writes);
            gpu.device.cmd_push_constants(cmd, self.layout, vk::ShaderStageFlags::COMPUTE, 0, &count.to_le_bytes());
            gpu.device.cmd_dispatch(cmd, count.min(GROUPS_X), count.div_ceil(GROUPS_X), 1);
        }
        // the barrier waits for what comes next (Encoder::after_models)
        enc.after_models = true;
        enc.pass_ended(gpu, cmd);
        if self.check.is_some() {
            self.read_back(gpu, enc, jobs, offset, count, target);
        }
    }

    /// Dev: the jobs' vertices copied out for the CPU to compare once the frame is done.
    fn read_back(&mut self, gpu: &Gpu, enc: &mut Encoder, jobs: &Buffer, offset: u64, count: u32, target: &Buffer) {
        let words = unsafe { std::slice::from_raw_parts(jobs.ptr().add(offset as usize) as *const u32, count as usize * JOB_WORDS) }.to_vec();
        let (mut lo, mut hi) = (u32::MAX, 0u32);
        for j in words.chunks_exact(JOB_WORDS) {
            if j[30] != NO_DESTINATION {
                lo = lo.min(j[30]);
                hi = hi.max(j[30] + j[22] * 9);
            }
        }
        if lo >= hi {
            return;
        }
        let bytes = (hi - lo) as u64 * 4;
        let readback = Buffer::with(gpu, vk::BufferUsageFlags::TRANSFER_DST, bytes, Place::Readback).expect("read-back buffer");
        enc.copy_buffer(gpu, target, lo as u64 * 4, &readback, 0, bytes);
        let check = self.check.as_mut().unwrap();
        check.pending.push(Pending { submit: enc.current, readback, from: lo, jobs: words });
        self.compare(enc);
    }

    /// Dev: compares the read-backs of finished frames.
    fn compare(&mut self, enc: &mut Encoder) {
        let Some(check) = self.check.as_mut() else { return };
        let done: Vec<Pending> = {
            let (done, wait): (Vec<Pending>, Vec<Pending>) = std::mem::take(&mut check.pending).into_iter().partition(|p| p.submit <= enc.completed);
            check.pending = wait;
            done
        };
        for p in done {
            let gpu_bytes = p.readback.ptr();
            for j in p.jobs.chunks_exact(JOB_WORDS) {
                if j[30] == NO_DESTINATION {
                    continue;
                }
                let n = j[22] as usize;
                let put_i = |r: &mut Vec<u8>, k: usize, v: i32| r[k * 4..k * 4 + 4].copy_from_slice(&v.to_le_bytes());
                let props = (if j[27] & 1 != 0 { 4 } else { 0 }) | (if j[27] & 2 != 0 { 8 } else { 0 });
                let pose = [j[0], j[1], j[2], 0, j[3], j[4], j[5], 0, j[6], j[7], j[8], 0, j[9], j[10], j[11], 1f32.to_bits()];
                let mut cpu = vec![0u8; n * models::VERTEX];
                let written = if j[27] & 4 == 0 {
                    // The same part through the CPU path: a record as NativeModels writes it.
                    let mut record = vec![0u8; models::RECORD];
                    put_i(&mut record, 0, j[28] as i32);
                    put_i(&mut record, 1, props);
                    put_i(&mut record, 2, 1);
                    for (k, w) in pose.iter().enumerate() {
                        put_i(&mut record, 4 + k, *w as i32);
                    }
                    for k in 0..9 {
                        put_i(&mut record, 20 + k, j[12 + k] as i32);
                    }
                    unsafe { models::vertices(&record, 1, j[26] as i32, j[25] as i32, j[29] as i32, cpu.as_mut_ptr()) }
                } else {
                    // The same quads through the CPU path (NativeItems' records), from what the GPU reads.
                    let mut header = vec![0u8; models::QUAD_HEADER];
                    put_i(&mut header, 0, props);
                    put_i(&mut header, 1, 1);
                    put_i(&mut header, 2, j[26] as i32);
                    put_i(&mut header, 3, j[25] as i32);
                    for (k, w) in pose.iter().enumerate() {
                        put_i(&mut header, 4 + k, *w as i32);
                    }
                    for k in 0..9 {
                        put_i(&mut header, 20 + k, j[12 + k] as i32);
                    }
                    let mesh = GPU_MESH.read().unwrap();
                    let quads = n / 4;
                    let mut records = vec![0u8; quads * models::QUAD];
                    for q in 0..quads {
                        let r = &mut records[q * models::QUAD..(q + 1) * models::QUAD];
                        let v0 = &mesh.data[j[21] as usize + q * 4];
                        for k in 0..4 {
                            let v = &mesh.data[j[21] as usize + q * 4 + k];
                            for c in 0..3 {
                                r[(k * 3 + c) * 4..(k * 3 + c) * 4 + 4].copy_from_slice(&v[c].to_le_bytes());
                            }
                            let uv = ((v[3] as u64) << 32) | v[4] as u64;
                            r[48 + k * 8..56 + k * 8].copy_from_slice(&uv.to_le_bytes());
                        }
                        for c in 0..3 {
                            r[80 + c * 4..84 + c * 4].copy_from_slice(&v0[5 + c].to_le_bytes());
                        }
                        let info = v0[8];
                        r[92..96].copy_from_slice(&(info & 15).to_le_bytes());
                        let index = (info >> 8) & 255;
                        let abgr = if info & 16 != 0 && index < (j[27] >> 8) & 3 { j[28 + index as usize] } else { u32::MAX };
                        // ABGR and ARGB swap the same two bytes.
                        let argb = (abgr & 0xFF00FF00) | ((abgr >> 16) & 0xFF) | ((abgr & 0xFF) << 16);
                        r[96..100].copy_from_slice(&argb.to_le_bytes());
                    }
                    unsafe { models::quads(&header, &records, quads, cpu.as_mut_ptr()) }
                };
                let at = (j[30] - p.from) as usize * 4;
                let gpu_part = unsafe { std::slice::from_raw_parts(gpu_bytes.add(at), n * models::VERTEX) };
                check.parts += 1;
                if written != n || gpu_part != &cpu[..] {
                    check.differ += 1;
                    if check.printed < 20 {
                        check.printed += 1;
                        let first = (0..n * models::VERTEX).find(|&k| gpu_part[k] != cpu[k]).unwrap_or(0);
                        note(format!(
                            "W [models gpu] {} {} differs from the CPU: {n} vertices, first at vertex {} byte {}: {} vs {}",
                            if j[27] & 4 != 0 { "item run at vertex" } else { "part" },
                            if j[27] & 4 != 0 { j[21] } else { j[28] },
                            first / models::VERTEX,
                            first % models::VERTEX,
                            gpu_part[first],
                            cpu[first]
                        ));
                    }
                }
                if check.parts % 20000 == 0 {
                    note(format!("I [models gpu] checked {} parts, {} differ", check.parts, check.differ));
                }
            }
            enc.defer(Doomed::Buffer(Box::new(p.readback)));
        }
    }

    pub fn destroy(&mut self, gpu: &Gpu) {
        let d = &gpu.device;
        unsafe {
            d.destroy_pipeline(self.pipeline, None);
            d.destroy_pipeline_layout(self.layout, None);
            d.destroy_descriptor_set_layout(self.set_layout, None);
            d.destroy_shader_module(self.module, None);
        }
        if let Some(b) = self.mesh.take() {
            Doomed::Buffer(Box::new(b)).destroy(gpu);
        }
        if let Some(c) = self.check.as_mut() {
            for p in c.pending.drain(..) {
                Doomed::Buffer(Box::new(p.readback)).destroy(gpu);
            }
        }
    }
}

fn note(text: String) {
    if let Ok(mut m) = super::device::MESSAGES.lock() {
        m.push(text);
    }
}
