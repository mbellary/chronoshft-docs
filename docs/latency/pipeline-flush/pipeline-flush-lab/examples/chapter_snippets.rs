#![allow(dead_code, clippy::needless_bool)]
mod snippet_0 {
    fn accepts(qty: u64, limit: u64) -> bool {
        if qty <= limit {
            true
        } else {
            false
        }
    }
}
mod snippet_1 {
    #[cold]
    fn report_gap(expected: u64, got: u64) {
        // In production: record a bounded event for another component.
        // Avoid synchronous formatting or disk I/O in the packet loop.
        let _ = (expected, got);
    }
}
mod snippet_3 {
    use std::sync::atomic::AtomicU64;

    #[repr(align(64))]
    struct PaddedCounter {
        value: AtomicU64,
    }

    struct Counters {
        received: PaddedCounter,
        transmitted: PaddedCounter,
    }
}
mod snippet_4 {
    use std::{io, net::TcpStream};

    fn configure(stream: &TcpStream) -> io::Result<()> {
        stream.set_nodelay(true)?;
        stream.set_nonblocking(true)?;
        Ok(())
    }
}
mod snippet_5 {
    use pipeline_flush_lab::Pending;
    use std::io;
    use std::net::TcpStream;

    fn output_turn(pending: &mut Pending<4096>, stream: &mut TcpStream) -> io::Result<bool> {
        pending.step(stream, 4) // no more than four write attempts this turn
    }
}
fn main() {}
