use std::mem::{align_of, size_of};

// Explicit C field ordering makes this teaching layout predictable.
#[repr(C)]
struct Particle {
    x: f64,
    y: f64,
    z: f64,
    mass: f64,
}

fn main() {
    let particles = vec![
        Particle {
            x: 1.0,
            y: 2.0,
            z: 3.0,
            mass: 4.0,
        },
        Particle {
            x: 5.0,
            y: 6.0,
            z: 7.0,
            mass: 8.0,
        },
    ];
    let x_positions = vec![1.0f64, 5.0];
    let from_records: f64 = particles.iter().map(|p| p.x).sum();
    let from_column: f64 = x_positions.iter().copied().sum();
    assert_eq!(from_records, from_column);
    println!(
        "Particle: size {}, alignment {}",
        size_of::<Particle>(),
        align_of::<Particle>()
    );
    println!(
        "f64:      size {}, alignment {}",
        size_of::<f64>(),
        align_of::<f64>()
    );
    println!("both x sums = {from_records}");
    println!(
        "record-buffer bytes: {}",
        particles.len() * size_of::<Particle>()
    );
    println!(
        "x-only buffer bytes: {}",
        x_positions.len() * size_of::<f64>()
    );
}
