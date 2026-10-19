use std::hint::black_box;
use std::time::Instant;

fn row_sum(data: &[u64], n: usize) -> u64 {
    let mut sum = 0u64;
    for row in 0..n {
        for col in 0..n {
            sum = sum.wrapping_add(data[row * n + col]);
        }
    }
    sum
}

fn column_sum(data: &[u64], n: usize) -> u64 {
    let mut sum = 0u64;
    for col in 0..n {
        for row in 0..n {
            sum = sum.wrapping_add(data[row * n + col]);
        }
    }
    sum
}

fn sample(data: &[u64], n: usize, f: fn(&[u64], usize) -> u64) -> f64 {
    let start = Instant::now();
    let result = f(black_box(data), black_box(n));
    black_box(result);
    start.elapsed().as_secs_f64() * 1_000.0
}

fn median(samples: &mut [f64]) -> f64 {
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    samples[samples.len() / 2]
}

fn main() {
    let quick = std::env::args().any(|arg| arg == "--quick");
    let n = if quick { 256 } else { 2048 };
    let data: Vec<u64> = (0..n * n).map(|i| (i % 251) as u64).collect();
    assert_eq!(row_sum(&data, n), column_sum(&data, n));
    let mut rows = Vec::new();
    let mut columns = Vec::new();
    // One warm-up of each path; allocation and initialization are not timed.
    black_box(row_sum(black_box(&data), n));
    black_box(column_sum(black_box(&data), n));
    for trial in 0..7 {
        if trial % 2 == 0 {
            rows.push(sample(&data, n, row_sum));
            columns.push(sample(&data, n, column_sum));
        } else {
            columns.push(sample(&data, n, column_sum));
            rows.push(sample(&data, n, row_sum));
        }
    }
    let row_ms = median(&mut rows);
    let col_ms = median(&mut columns);
    println!(
        "{n} x {n}; buffer = {:.2} MiB",
        data.len() as f64 * 8.0 / 1_048_576.0
    );
    println!("row traversal median:    {row_ms:.3} ms");
    println!("column traversal median: {col_ms:.3} ms");
    println!("column / row elapsed-time ratio: {:.2}", col_ms / row_ms);
}
