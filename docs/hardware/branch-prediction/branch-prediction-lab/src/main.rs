use std::{env, hint::black_box, time::Instant};

// Educational policy; not a complete exchange order or production risk engine.
#[derive(Clone, Copy, Debug)]
struct Order {
    price_ticks: u64,
    qty: u32,
    enabled: bool,
}
const MAX_QTY: u32 = 1_000;
const MAX_PRICE: u64 = 10_000;

#[inline(always)]
fn branch(o: &Order) -> bool {
    if !o.enabled {
        return false;
    }
    if o.qty == 0 {
        return false;
    }
    if o.qty > MAX_QTY {
        return false;
    }
    if o.price_ticks > MAX_PRICE {
        return false;
    }
    true
}

#[inline(always)]
fn eager(o: &Order) -> bool {
    // & evaluates every boolean operand; && short-circuits.
    o.enabled & (o.qty != 0) & (o.qty <= MAX_QTY) & (o.price_ticks <= MAX_PRICE)
}

const ALLOW: [bool; 16] = [
    false, false, false, false, false, false, false, false, false, false, false, false, false,
    false, false, true,
];

#[inline(always)]
fn table(o: &Order) -> bool {
    let flags = usize::from(o.enabled)
        | (usize::from(o.qty != 0) << 1)
        | (usize::from(o.qty <= MAX_QTY) << 2)
        | (usize::from(o.price_ticks <= MAX_PRICE) << 3);
    ALLOW[flags] // flags is provably within 0..16; no unchecked access needed.
}

fn next(rng: &mut u64) -> u64 {
    *rng ^= *rng << 13;
    *rng ^= *rng >> 7;
    *rng ^= *rng << 17;
    *rng
}

fn make_data(profile: &str, n: usize) -> Vec<Order> {
    let mut rng = 0x1234_5678_9abc_def0;
    (0..n)
        .map(|i| {
            let r = next(&mut rng);
            match profile {
                "steady" => Order {
                    price_ticks: 9_999,
                    qty: 100,
                    enabled: true,
                },
                // One unpredictable predicate with approximately 50% approval.
                "mixed" => Order {
                    price_ticks: 9_999,
                    qty: 100,
                    enabled: r & 1 != 0,
                },
                // Many different rejection causes, drawn from the same PRNG stream.
                "chaotic" => Order {
                    price_ticks: if r & 1 == 0 { 9_999 } else { 10_001 },
                    qty: match (r >> 1) & 3 {
                        0 => 0,
                        1 => 1_001,
                        _ => 100,
                    },
                    enabled: r & 8 != 0,
                },
                // Alternating 4096-message regimes; synthetic, not historical replay.
                "regime" => {
                    if (i / 4096) % 2 == 0 {
                        Order {
                            price_ticks: 9_999,
                            qty: 100,
                            enabled: true,
                        }
                    } else {
                        Order {
                            price_ticks: 9_999,
                            qty: 100,
                            enabled: r & 1 != 0,
                        }
                    }
                }
                _ => panic!("profile must be steady, mixed, chaotic, or regime"),
            }
        })
        .collect()
}

#[inline(never)]
fn scan<F: Fn(&Order) -> bool>(data: &[Order], f: &F) -> u64 {
    data.iter().map(|o| u64::from(f(o))).sum()
}

fn verify() {
    for enabled in [false, true] {
        for qty in [0, 1, MAX_QTY, MAX_QTY + 1, u32::MAX] {
            for price_ticks in [0, MAX_PRICE, MAX_PRICE + 1, u64::MAX] {
                let o = Order {
                    price_ticks,
                    qty,
                    enabled,
                };
                assert_eq!(branch(&o), eager(&o), "{o:?}");
                assert_eq!(branch(&o), table(&o), "{o:?}");
            }
        }
    }
    for profile in ["steady", "mixed", "chaotic", "regime"] {
        for o in make_data(profile, 100_000) {
            assert_eq!(branch(&o), eager(&o));
            assert_eq!(branch(&o), table(&o));
        }
    }
    println!("verified boundary cases and 400000 synthetic records");
}

fn bench<F: Fn(&Order) -> bool>(data: &[Order], passes: usize, f: F) {
    for _ in 0..8 {
        black_box(scan(black_box(data), &f));
    }
    let start = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..passes {
        checksum = checksum.wrapping_add(scan(black_box(data), &f));
    }
    let elapsed = start.elapsed();
    let events = data.len() as f64 * passes as f64;
    println!(
        "events={events:.0} elapsed_ns={} amortized_ns_per_event={:.3} checksum={}",
        elapsed.as_nanos(),
        elapsed.as_nanos() as f64 / events,
        black_box(checksum)
    );
}

fn samples<F: Fn(&Order) -> bool>(data: &[Order], passes: usize, f: F) {
    for _ in 0..8 {
        black_box(scan(black_box(data), &f));
    }
    let count = data.len().checked_mul(passes).expect("too many samples");
    let mut values = Vec::with_capacity(count);
    for _ in 0..passes {
        for order in data {
            let o = black_box(order);
            let start = Instant::now();
            black_box(f(o));
            let ns = start.elapsed().as_nanos();
            values.push(ns); // Outside measured region, but affects next sample.
        }
    }
    values.sort_unstable(); // Outside all measured regions.
    let q = |p: f64| {
        values[((p * count as f64).ceil() as usize)
            .saturating_sub(1)
            .min(count - 1)]
    };
    println!(
        "instrumented_samples={count} p50_ns={} p99_ns={} p999_ns={} p9999_ns={} max_ns={}",
        q(0.5),
        q(0.99),
        q(0.999),
        q(0.9999),
        values[count - 1]
    );
    println!("Includes clock and black_box costs. Not an arrival-driven latency test.");
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.get(1).map(String::as_str) == Some("verify") {
        verify();
        return;
    }
    if args.len() != 6 {
        eprintln!("Usage: branch-prediction-lab verify\n       branch-prediction-lab <bench|samples> <branch|eager|table> <steady|mixed|chaotic|regime> <records> <passes>");
        std::process::exit(2);
    }
    let n: usize = args[4].parse().expect("records must be a positive integer");
    let passes: usize = args[5].parse().expect("passes must be a positive integer");
    assert!(n > 0 && passes > 0);
    let data = make_data(&args[3], n);
    // Select implementation outside the timed loops; no function pointer per record.
    match (args[1].as_str(), args[2].as_str()) {
        ("bench", "branch") => bench(&data, passes, branch),
        ("bench", "eager") => bench(&data, passes, eager),
        ("bench", "table") => bench(&data, passes, table),
        ("samples", "branch") => samples(&data, passes, branch),
        ("samples", "eager") => samples(&data, passes, eager),
        ("samples", "table") => samples(&data, passes, table),
        _ => panic!("invalid mode or implementation"),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn policy_equivalence() {
        super::verify();
    }
}
