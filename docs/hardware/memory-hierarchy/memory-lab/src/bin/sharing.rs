use std::sync::atomic::{AtomicU64, Ordering};

// Demonstrates layout only. A 64-byte assumption is not portable hardware discovery.
#[repr(align(64))]
struct PaddedCounter(AtomicU64);

fn main() {
    let counters = [
        PaddedCounter(AtomicU64::new(0)),
        PaddedCounter(AtomicU64::new(0)),
    ];
    std::thread::scope(|scope| {
        for counter in &counters {
            scope.spawn(move || {
                // Accumulate privately, then publish once.
                let mut local = 0u64;
                for i in 0..100_000u64 {
                    local = local.wrapping_add(i);
                }
                counter.0.store(local, Ordering::Relaxed);
            });
        }
    });
    let total: u64 = counters.iter().map(|c| c.0.load(Ordering::Relaxed)).sum();
    assert_eq!(total, 9_999_900_000);
    println!("combined total = {total}");
    println!(
        "counter wrapper bytes = {}",
        std::mem::size_of::<PaddedCounter>()
    );
}
