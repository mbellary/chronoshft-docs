use std::hint::black_box;
use std::mem::size_of;
use std::time::Instant;

fn random(state: &mut u64) -> u64 {
    // A deterministic generator for an experiment, not cryptography.
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

fn make_cycle(n: usize) -> Vec<usize> {
    assert!(n > 0);
    let mut order: Vec<usize> = (0..n).collect();
    let mut state = 0x1234_5678_9abc_def0u64;
    for i in (1..n).rev() {
        // Tiny modulo bias is acceptable for this educational permutation.
        let j = (random(&mut state) % (i as u64 + 1)) as usize;
        order.swap(i, j);
    }
    let mut next = vec![0; n];
    for i in 0..n {
        next[order[i]] = order[(i + 1) % n];
    }
    next
}

fn walk(next: &[usize], mut index: usize, steps: usize) -> usize {
    for _ in 0..steps {
        index = next[index];
    }
    index
}

fn main() {
    let quick = std::env::args().any(|arg| arg == "--quick");
    let sizes_kib: &[usize] = if quick {
        &[8, 256, 4096]
    } else {
        &[4, 16, 32, 64, 256, 1024, 4096, 16384, 65536]
    };
    println!("KiB of index buffer | median ns / dependent step");
    for &kib in sizes_kib {
        let n = kib * 1024 / size_of::<usize>();
        let next = make_cycle(n);
        let steps = if quick {
            200_000.max(n)
        } else {
            2_000_000.max(n)
        };
        // Touch the entire cycle before timing. Large buffers still cannot fit in L1.
        black_box(walk(black_box(&next), black_box(0), n));
        let mut times = Vec::new();
        for _ in 0..5 {
            let start = Instant::now();
            let end = walk(black_box(&next), black_box(0), black_box(steps));
            black_box(end);
            times.push(start.elapsed().as_secs_f64() * 1e9 / steps as f64);
        }
        times.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!("{kib:>19} | {:>26.2}", times[2]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_visits_every_element_once_before_returning() {
        for n in [1, 2, 17, 1024] {
            let next = make_cycle(n);
            let mut seen = vec![false; n];
            let mut index = 0;
            for _ in 0..n {
                assert!(!seen[index], "cycle returned too early");
                seen[index] = true;
                index = next[index];
            }
            assert_eq!(index, 0);
            assert!(seen.iter().all(|&value| value));
        }
    }
}
