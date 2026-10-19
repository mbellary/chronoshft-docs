use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;

struct Published {
    qty: AtomicU64,
    ready: AtomicBool,
}

fn main() {
    let p = Published {
        qty: AtomicU64::new(0),
        ready: AtomicBool::new(false),
    };
    thread::scope(|scope| {
        let producer = &p;
        scope.spawn(move || {
            producer.qty.store(42, Ordering::Relaxed);
            producer.ready.store(true, Ordering::Release);
        });
        while !p.ready.load(Ordering::Acquire) {
            std::hint::spin_loop();
        }
        assert_eq!(p.qty.load(Ordering::Relaxed), 42);
    });
}
