//! Safe educational building blocks; no exchange connection or trading actions.
use std::io::{self, Write};

pub const LEVELS: usize = 8;
pub const MAX_REPLAY: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Update {
    pub session: u64,
    pub seq: u64,
    pub level: usize,
    pub qty: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Live,
    Recovering,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Applied,
    Duplicate,
    RecoveryRequired,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryError {
    TooMuchReplay,
    WrongSession,
    Gap,
    BadLevel,
    SequenceOverflow,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Engine {
    session: u64,
    next: u64,
    quantities: [u64; LEVELS],
    mode: Mode,
}

impl Engine {
    /// Snapshot is authoritative through `through`, inclusive, in this session.
    /// Caller must validate its provenance and sequence boundary before calling.
    pub fn from_snapshot(
        session: u64,
        through: u64,
        quantities: [u64; LEVELS],
    ) -> Result<Self, RecoveryError> {
        Ok(Self {
            session,
            next: through
                .checked_add(1)
                .ok_or(RecoveryError::SequenceOverflow)?,
            quantities,
            mode: Mode::Live,
        })
    }
    pub fn mode(&self) -> Mode {
        self.mode
    }
    pub fn next(&self) -> u64 {
        self.next
    }
    pub fn quantities(&self) -> &[u64; LEVELS] {
        &self.quantities
    }
    pub fn can_trade(&self) -> bool {
        self.mode == Mode::Live
    }

    pub fn ingest(&mut self, u: Update) -> Outcome {
        if self.mode != Mode::Live {
            return Outcome::RecoveryRequired;
        }
        if u.session != self.session || u.level >= LEVELS {
            return self.stop();
        }
        if u.seq < self.next {
            return Outcome::Duplicate;
        }
        if u.seq != self.next {
            return self.stop();
        }
        let Some(next) = self.next.checked_add(1) else {
            return self.stop();
        };
        self.quantities[u.level] = u.qty;
        self.next = next;
        Outcome::Applied
    }
    #[cold]
    fn stop(&mut self) -> Outcome {
        self.mode = Mode::Recovering;
        Outcome::RecoveryRequired
    }
    /// `cutover` is the last sequence that must be present before publication.
    /// Replay is in feed order, bounded, and all entries belong to snapshot session.
    /// Successful publication requires exactly contiguous coverage through cutover.
    pub fn recover(
        &mut self,
        session: u64,
        through: u64,
        quantities: [u64; LEVELS],
        replay: &[Update],
        cutover: u64,
    ) -> Result<(), RecoveryError> {
        // Failure must keep trading disabled and the prior book intact.
        self.mode = Mode::Recovering;
        if replay.len() > MAX_REPLAY {
            return Err(RecoveryError::TooMuchReplay);
        }
        if cutover < through {
            return Err(RecoveryError::Gap);
        }
        let expected_next = cutover
            .checked_add(1)
            .ok_or(RecoveryError::SequenceOverflow)?;
        let mut candidate = Self::from_snapshot(session, through, quantities)?;
        for &u in replay {
            if u.session != session {
                return Err(RecoveryError::WrongSession);
            }
            if u.level >= LEVELS {
                return Err(RecoveryError::BadLevel);
            }
            if u.seq <= through {
                continue;
            } // already covered by snapshot
            if u.seq != candidate.next {
                return Err(RecoveryError::Gap);
            }
            if candidate.ingest(u) != Outcome::Applied {
                return Err(RecoveryError::SequenceOverflow);
            }
        }
        if candidate.next != expected_next {
            return Err(RecoveryError::Gap);
        }
        *self = candidate; // single-thread owner publishes complete candidate
        Ok(())
    }
}

/// One owned frame. An incomplete frame prevents admitting a replacement frame.
/// Each step makes at most `max_calls` write attempts, including interruptions.
pub struct Pending<const N: usize> {
    bytes: [u8; N],
    len: usize,
    sent: usize,
}
#[derive(Debug, PartialEq, Eq)]
pub enum EnqueueError {
    Busy,
    TooLarge,
    Empty,
}
impl<const N: usize> Default for Pending<N> {
    fn default() -> Self {
        Self {
            bytes: [0; N],
            len: 0,
            sent: 0,
        }
    }
}
impl<const N: usize> Pending<N> {
    pub fn enqueue(&mut self, frame: &[u8]) -> Result<(), EnqueueError> {
        if !self.is_empty() {
            return Err(EnqueueError::Busy);
        }
        if frame.is_empty() {
            return Err(EnqueueError::Empty);
        }
        if frame.len() > N {
            return Err(EnqueueError::TooLarge);
        }
        self.bytes[..frame.len()].copy_from_slice(frame);
        self.len = frame.len();
        self.sent = 0;
        Ok(())
    }
    pub fn is_empty(&self) -> bool {
        self.sent == self.len
    }
    pub fn remaining(&self) -> usize {
        self.len - self.sent
    }
    pub fn step<W: Write>(&mut self, writer: &mut W, max_calls: usize) -> io::Result<bool> {
        for _ in 0..max_calls {
            if self.is_empty() {
                break;
            }
            match writer.write(&self.bytes[self.sent..self.len]) {
                Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
                Ok(n) => self.sent += n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e),
            }
        }
        Ok(self.is_empty())
    }
}

/// Open-loop virtual-time queue model, not a NIC or CPU measurement.
/// Arrival schedule is independent of processing completion.
pub fn queue_model(
    n: usize,
    interval_ns: u64,
    service_ns: u64,
    stall_at: usize,
    stall_ns: u64,
) -> Vec<u64> {
    let mut done = 0u64;
    (0..n)
        .map(|i| {
            let arrival = (i as u64) * interval_ns;
            let start = done.max(arrival);
            done = start + service_ns + if i == stall_at { stall_ns } else { 0 };
            done - arrival
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn u(seq: u64) -> Update {
        Update {
            session: 7,
            seq,
            level: 0,
            qty: seq,
        }
    }
    fn engine() -> Engine {
        Engine::from_snapshot(7, 9, [0; LEVELS]).unwrap()
    }
    #[test]
    fn gap_disables_and_does_not_apply_later_updates() {
        let mut e = engine();
        assert_eq!(e.ingest(u(10)), Outcome::Applied);
        assert_eq!(e.ingest(u(12)), Outcome::RecoveryRequired);
        assert_eq!(e.ingest(u(11)), Outcome::RecoveryRequired);
        assert_eq!(e.quantities()[0], 10);
        assert!(!e.can_trade());
    }
    #[test]
    fn old_duplicate_does_not_mutate() {
        let mut e = engine();
        e.ingest(u(10));
        let before = e.clone();
        assert_eq!(e.ingest(u(10)), Outcome::Duplicate);
        assert_eq!(e, before);
    }
    #[test]
    fn recovery_is_contiguous_and_atomic_for_owner() {
        let mut e = engine();
        e.ingest(u(12));
        e.recover(7, 10, [5; LEVELS], &[u(9), u(11), u(12)], 12)
            .unwrap();
        assert!(e.can_trade());
        assert_eq!(e.next(), 13);
        assert_eq!(e.quantities()[0], 12);
        assert_eq!(e.quantities()[1], 5);
    }
    #[test]
    fn failed_recovery_keeps_old_book_and_stops_trading() {
        let mut e = engine();
        let old = *e.quantities();
        assert_eq!(
            e.recover(7, 10, [5; LEVELS], &[u(12)], 12),
            Err(RecoveryError::Gap)
        );
        assert_eq!(*e.quantities(), old);
        assert!(!e.can_trade());
        assert_eq!(
            e.recover(7, 10, [5; LEVELS], &[u(11)], 12),
            Err(RecoveryError::Gap)
        );
    }
    #[test]
    fn rejects_session_level_overflow_and_capacity_errors() {
        let mut e = engine();
        assert_eq!(
            e.ingest(Update {
                session: 8,
                ..u(10)
            }),
            Outcome::RecoveryRequired
        );
        assert_eq!(
            e.recover(
                7,
                10,
                [0; LEVELS],
                &[Update {
                    session: 8,
                    ..u(11)
                }],
                11
            ),
            Err(RecoveryError::WrongSession)
        );
        assert_eq!(
            e.recover(
                7,
                10,
                [0; LEVELS],
                &[Update {
                    level: LEVELS,
                    ..u(11)
                }],
                11
            ),
            Err(RecoveryError::BadLevel)
        );
        assert_eq!(
            e.recover(7, 10, [0; LEVELS], &[u(11); MAX_REPLAY + 1], 11),
            Err(RecoveryError::TooMuchReplay)
        );
        assert_eq!(
            Engine::from_snapshot(7, u64::MAX, [0; LEVELS]),
            Err(RecoveryError::SequenceOverflow)
        );
        let mut e = Engine::from_snapshot(7, u64::MAX - 1, [0; LEVELS]).unwrap();
        assert_eq!(e.ingest(u(u64::MAX)), Outcome::RecoveryRequired);
        assert_eq!(e.quantities()[0], 0);
    }
    struct Script {
        calls: usize,
        bytes: Vec<u8>,
    }
    impl Write for Script {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.calls += 1;
            match self.calls {
                1 => {
                    self.bytes.extend_from_slice(&b[..2]);
                    Ok(2)
                }
                2 => Err(io::ErrorKind::WouldBlock.into()),
                3 => Err(io::ErrorKind::Interrupted.into()),
                _ => {
                    self.bytes.extend_from_slice(b);
                    Ok(b.len())
                }
            }
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    #[test]
    fn partial_writes_preserve_offset_and_call_budget() {
        let mut p = Pending::<8>::default();
        p.enqueue(b"abcdef").unwrap();
        let mut w = Script {
            calls: 0,
            bytes: vec![],
        };
        assert!(!p.step(&mut w, 2).unwrap());
        assert_eq!(p.remaining(), 4);
        assert_eq!(p.enqueue(b"new"), Err(EnqueueError::Busy));
        assert!(!p.step(&mut w, 1).unwrap());
        assert_eq!(w.calls, 3);
        assert!(p.step(&mut w, 1).unwrap());
        assert_eq!(w.bytes, b"abcdef");
    }
    #[test]
    fn writer_zero_and_frame_bounds() {
        let mut p = Pending::<4>::default();
        assert_eq!(p.enqueue(b""), Err(EnqueueError::Empty));
        assert_eq!(p.enqueue(b"12345"), Err(EnqueueError::TooLarge));
        p.enqueue(b"1234").unwrap();
        let mut empty: &mut [u8] = &mut [];
        assert_eq!(
            p.step(&mut empty, 1).unwrap_err().kind(),
            io::ErrorKind::WriteZero
        );
        assert_eq!(p.remaining(), 4);
    }
    #[test]
    fn queue_stall_catches_up_only_with_spare_capacity() {
        assert_eq!(queue_model(5, 100, 30, 1, 200), vec![30, 230, 160, 90, 30]);
        assert_eq!(queue_model(4, 20, 30, 1, 200), vec![30, 240, 250, 260]);
    }
}
