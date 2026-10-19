use pipeline_flush_lab::{queue_model, Engine, Update, LEVELS};
use std::{hint::black_box, time::Instant};

#[inline(never)]
fn branch_work(data: &[u64]) -> u64 {
    let mut sum = 0u64;
    for &v in data {
        let v = black_box(v);
        if v & 1 == 0 {
            sum = sum.wrapping_add(v.rotate_left(7));
        } else {
            sum = sum.wrapping_add(v.rotate_right(11));
        }
    }
    sum
}
fn branch(pattern: &str) {
    const N: usize = 262_144;
    let mut seed = 0x1234_5678_9abc_def0u64;
    let data: Vec<u64> = (0..N)
        .map(|i| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            match pattern {
                "same" => seed & !1,
                "alternating" => (seed & !1) | (i as u64 & 1),
                _ => seed,
            }
        })
        .collect();
    for _ in 0..8 {
        black_box(branch_work(black_box(&data)));
    }
    let mut samples = Vec::with_capacity(40);
    for _ in 0..40 {
        let start = Instant::now();
        black_box(branch_work(black_box(&data)));
        samples.push(start.elapsed().as_nanos() as f64 / N as f64);
    }
    samples.sort_by(f64::total_cmp);
    println!(
        "pattern={pattern}; 40 batches; ns/item median={:.3}; slowest batch ns/item={:.3}",
        samples[20], samples[39]
    );
    println!("Batch throughput experiment, NOT per-event tail latency or isolated misprediction cost. Inspect assembly for a conditional branch.");
}
fn queue() {
    for interval in [100, 20] {
        let mut values = queue_model(20_000, interval, 30, 5_000, 200);
        values.sort_unstable();
        let q = |numerator: usize, denominator: usize| {
            values[(values.len() * numerator).div_ceil(denominator) - 1]
        };
        println!("SIMULATED ns: arrival_interval={interval} service=30 stall=200; p50={} p99={} p99.99={} max={}", q(1,2), q(99,100), q(9999,10000), values.last().unwrap());
    }
}
fn recovery() {
    let mut e = Engine::from_snapshot(7, 9, [0; LEVELS]).unwrap();
    for seq in [10, 12, 11] {
        let out = e.ingest(Update {
            session: 7,
            seq,
            level: 0,
            qty: seq,
        });
        println!(
            "seq={seq} outcome={out:?} mode={:?} can_trade={}",
            e.mode(),
            e.can_trade()
        );
    }
    let replay = [Update {
        session: 7,
        seq: 12,
        level: 0,
        qty: 12,
    }];
    e.recover(7, 11, [11; LEVELS], &replay, 12).unwrap();
    println!(
        "snapshot through 11 + replay 12: mode={:?}, next={}, qty[0]={}",
        e.mode(),
        e.next(),
        e.quantities()[0]
    );
}
fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("branch") => branch(args.get(2).map(String::as_str).unwrap_or("random")),
        Some("queue") => queue(),
        Some("recovery") => recovery(),
        _ => eprintln!(
            "Usage: pipeline-flush-lab branch [same|alternating|random] | queue | recovery"
        ),
    }
}
