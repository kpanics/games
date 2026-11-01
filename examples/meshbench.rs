//! The section mesher on its own: tables and section inputs dumped by the game (CURSED_DEV_MESHBENCH, run/meshbench/),
//! meshed again and again — to make the mesher faster without starting the game.
//!     cargo run --release --example meshbench -- ..\run\meshbench

use cursed_recode::world::{blocks, mesher};
use std::time::Instant;

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "../run/meshbench".into());
    blocks::load(&std::fs::read(format!("{dir}/tables.bin")).expect("tables.bin"));
    let tables = blocks::current().unwrap();
    let mut sections = Vec::new();
    for i in 0.. {
        let Ok(b) = std::fs::read(format!("{dir}/section{i}.bin")) else { break };
        sections.push(b);
    }
    println!("{} sections", sections.len());
    struct Prepared {
        states: Vec<i32>,
        light: Vec<u8>,
        cardinal: [f32; 6],
        origin: [i32; 3],
        flags: i32,
    }
    let prepared: Vec<Prepared> = sections
        .iter()
        .map(|b| {
            let h = |i: usize| i32::from_le_bytes(b[i * 4..i * 4 + 4].try_into().unwrap());
            let body = &b[16..];
            let cardinal: Vec<f32> = body[40000..40024].chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect();
            Prepared {
                states: body[..32000].chunks_exact(4).map(|c| i32::from_le_bytes(c.try_into().unwrap())).collect(),
                light: body[32000..40000].to_vec(),
                cardinal: cardinal.try_into().unwrap(),
                origin: [h(0), h(1), h(2)],
                flags: h(3),
            }
        })
        .collect();
    let mut out = mesher::Output::default();
    let mut quads = 0usize;
    let rounds = 200;
    let start = Instant::now();
    for round in 0..rounds {
        for p in &prepared {
            let input = mesher::Input {
                states: &p.states,
                light: &p.light,
                origin: p.origin,
                ambient_occlusion: p.flags & 1 != 0 && std::env::var("NOAO").is_err(),
                cutout_leaves: p.flags & 2 != 0,
                cardinal: p.cardinal,
            };
            let mut tint = |_: u32, _: i32, _: i32| Some(0xFF48B518u32 as i32);
            mesher::mesh(&tables, &input, &mut tint, &mut out);
            if round == 0 {
                quads += out.layers.iter().map(|l| l.len() / mesher::VERTEX / 4).sum::<usize>();
            }
        }
    }
    let per = start.elapsed().as_secs_f64() * 1e6 / (rounds * prepared.len()) as f64;
    println!("{quads} quads in all; {per:.1} us per section");
}
