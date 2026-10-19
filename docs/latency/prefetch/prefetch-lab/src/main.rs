//! Educational batch benchmark. Does NOT measure per-event tail latency.
use std::{env, hint::black_box, io::Write, time::Instant};

// A deliberately padded record for the 64-byte-line teaching experiment.
// This is not a recommended layout for every trading application.
#[repr(C, align(64))]
struct Book {
    ask_ticks: u64,
    available_qty: u64,
    unused: [u64; 6],
}

#[derive(Clone, Copy)]
struct Request {
    symbol: usize,
    limit_ticks: u64,
    qty: u64,
}

// Receiving a shared reference ensures that this address names a live object.
// No raw-pointer arithmetic or dereference is necessary.
#[inline(always)]
fn prefetch_read<T>(value: &T) {
    #[cfg(target_arch = "x86_64")]
    {
        use std::arch::x86_64::{_MM_HINT_T0, _mm_prefetch};
        // Current Rust exposes this hint as safe. The unsafe block also supports
        // older stable versions where the intrinsic was declared unsafe.
        #[allow(unused_unsafe)]
        unsafe {
            _mm_prefetch::<_MM_HINT_T0>((value as *const T).cast::<i8>());
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    let _ = value; // Explicit portable fallback: no software hint.
}

#[inline(always)]
fn accepted_qty(book: &Book, request: &Request) -> u64 {
    // A toy read-only decision; no positions, orders or risk state are updated.
    if book.ask_ticks <= request.limit_ticks {
        request.qty.min(book.available_qty)
    } else {
        0
    }
}

#[inline(never)]
fn process<const DISTANCE: usize>(books: &[Book], requests: &[Request]) -> u64 {
    let mut checksum = 0u64;
    for (i, request) in requests.iter().enumerate() {
        // DISTANCE=0 means no hint, not a prefetch of the current item.
        if DISTANCE != 0 {
            if let Some(future) = i.checked_add(DISTANCE).and_then(|j| requests.get(j)) {
                // get() also makes malformed lookahead symbols harmless here.
                if let Some(book) = books.get(future.symbol) {
                    prefetch_read(book);
                }
            }
        }
        // Normal indexing keeps Rust's bounds check for the demand access.
        // A malformed current symbol is a caller error and will panic.
        let qty = accepted_qty(&books[request.symbol], request);
        checksum = checksum.wrapping_add(qty);
    }
    checksum
}

fn random(state: &mut u64) -> u64 {
    // Reproducible workload generator, not a cryptographic RNG.
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    // Nearest-rank quantile. No interpolated claim of nanosecond precision.
    let rank = ((p * sorted.len() as f64).ceil() as usize).max(1);
    sorted[rank - 1]
}

fn measure<const D: usize>(
    books: &[Book],
    requests: &[Request],
    repeats: usize,
    csv: Option<&str>,
) {
    const BATCH: usize = 4096;
    let chunks = requests.len().div_ceil(BATCH);
    let mut samples =
        Vec::with_capacity(chunks.checked_mul(repeats).expect("sample count overflow"));
    black_box(process::<D>(black_box(books), black_box(requests))); // warmup
    let mut checksum = 0u64;
    let mut total_ns = 0u128;
    let mut total_items = 0u128;
    for _ in 0..repeats {
        for batch in requests.chunks(BATCH) {
            let start = Instant::now();
            checksum = checksum.wrapping_add(process::<D>(black_box(books), black_box(batch)));
            black_box(checksum);
            let elapsed = start.elapsed().as_nanos();
            samples.push(elapsed as f64 / batch.len() as f64);
            total_ns += elapsed;
            total_items += batch.len() as u128;
        }
    }
    // All output and sorting happen after the timed experiment.
    if let Some(path) = csv {
        let mut file = std::io::BufWriter::new(std::fs::File::create(path).expect("create CSV"));
        writeln!(file, "sample,batch_average_ns_per_item").unwrap();
        for (i, sample) in samples.iter().enumerate() {
            writeln!(file, "{i},{sample:.6}").unwrap();
        }
    }
    samples.sort_unstable_by(f64::total_cmp);
    println!(
        "distance={D} batch_samples={} checksum={checksum}",
        samples.len()
    );
    println!(
        "timed_batch_mean_ns_per_item={:.3}",
        total_ns as f64 / total_items as f64
    );
    println!(
        "BATCH AVERAGES ONLY: p50={:.3} p99={:.3} max={:.3} ns/item",
        percentile(&samples, 0.50),
        percentile(&samples, 0.99),
        samples.last().unwrap()
    );
    println!("These are NOT per-event percentiles or tick-to-trade measurements.");
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.iter().any(|s| s == "--help") {
        println!(
            "prefetch-lab [distance=0] [books=1048576] [requests=131072] [repeats=40] [random|sequential|hot=random] [samples.csv]"
        );
        println!("Distances: 0,1,2,4,8,16,32,64. CSV is optional; it is overwritten if present.");
        return;
    }
    let number = |index: usize, default: usize| -> usize {
        args.get(index)
            .map(|s| s.parse().expect("expected unsigned integer"))
            .unwrap_or(default)
    };
    let distance = number(1, 0);
    let book_count = number(2, 1 << 20);
    let request_count = number(3, 1 << 17);
    let repeats = number(4, 40);
    let pattern = args.get(5).map(String::as_str).unwrap_or("random");
    let csv = args.get(6).map(String::as_str);
    assert!(
        book_count > 0 && request_count > 0 && repeats > 0,
        "counts must be positive"
    );
    assert!(
        matches!(distance, 0 | 1 | 2 | 4 | 8 | 16 | 32 | 64),
        "unsupported distance"
    );
    assert!(
        matches!(pattern, "random" | "sequential" | "hot"),
        "unknown pattern"
    );
    let books: Vec<Book> = (0..book_count)
        .map(|i| Book {
            ask_ticks: 10_000 + (i % 101) as u64,
            available_qty: 10 + (i % 89) as u64,
            unused: [0; 6],
        })
        .collect();
    let mut seed = 0x1234_5678_9abc_def0;
    let requests: Vec<Request> = (0..request_count)
        .map(|i| {
            let r = random(&mut seed);
            let symbol = match pattern {
                "sequential" => i % book_count,
                "hot" => (r as usize) % book_count.min(64),
                _ => (r as usize) % book_count,
            };
            Request {
                symbol,
                limit_ticks: 10_050,
                qty: 1 + ((r >> 32) % 50),
            }
        })
        .collect();
    println!(
        "arch={} software_hint={} books={} bytes pattern={pattern}",
        env::consts::ARCH,
        cfg!(target_arch = "x86_64"),
        books.len() * size_of::<Book>()
    );
    match distance {
        0 => measure::<0>(&books, &requests, repeats, csv),
        1 => measure::<1>(&books, &requests, repeats, csv),
        2 => measure::<2>(&books, &requests, repeats, csv),
        4 => measure::<4>(&books, &requests, repeats, csv),
        8 => measure::<8>(&books, &requests, repeats, csv),
        16 => measure::<16>(&books, &requests, repeats, csv),
        32 => measure::<32>(&books, &requests, repeats, csv),
        64 => measure::<64>(&books, &requests, repeats, csv),
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decision_and_all_prefetch_distances_agree() {
        let books = vec![
            Book {
                ask_ticks: 100,
                available_qty: 7,
                unused: [0; 6],
            },
            Book {
                ask_ticks: 101,
                available_qty: 9,
                unused: [0; 6],
            },
        ];
        // Accept 7, reject 0, accept 3: exercises both decisions and qty cap.
        let requests = vec![
            Request {
                symbol: 0,
                limit_ticks: 100,
                qty: 10,
            },
            Request {
                symbol: 1,
                limit_ticks: 100,
                qty: 10,
            },
            Request {
                symbol: 1,
                limit_ticks: 101,
                qty: 3,
            },
        ];
        assert_eq!(process::<0>(&books, &requests), 10);
        assert_eq!(process::<1>(&books, &requests), 10);
        assert_eq!(process::<2>(&books, &requests), 10);
        assert_eq!(process::<4>(&books, &requests), 10);
        assert_eq!(process::<8>(&books, &requests), 10);
        assert_eq!(process::<16>(&books, &requests), 10);
        assert_eq!(process::<32>(&books, &requests), 10);
        assert_eq!(process::<64>(&books, &requests), 10);
        assert_eq!(process::<{ usize::MAX }>(&books, &requests), 10);
        assert_eq!(process::<8>(&[], &[]), 0);
    }

    #[test]
    fn teaching_record_is_one_assumed_cache_line() {
        assert_eq!(size_of::<Book>(), 64);
        assert_eq!(align_of::<Book>(), 64);
    }
}
