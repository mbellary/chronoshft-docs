use std::mem::{align_of, offset_of, size_of};

#[repr(C)]
struct Record {
    tag: u8,
    count: u32,
    code: u16,
}

#[repr(C)]
struct CompactRecord {
    count: u32,
    code: u16,
    tag: u8,
}

#[allow(dead_code)]
struct RustRecord {
    tag: u8,
    count: u32,
    code: u16,
}

#[repr(C, packed)]
struct PackedRecord {
    tag: u8,
    count: u32,
    code: u16,
}

#[repr(C, align(16))]
struct AlignedByte(u8);

#[repr(C)]
struct Inner {
    number: u32,
    flag: u8,
}

#[repr(C)]
struct Outer {
    inner: Inner,
    next: u8,
}

fn main() {
    println!("u8: size={}, alignment={}", size_of::<u8>(), align_of::<u8>());
    println!("u16: size={}, alignment={}", size_of::<u16>(), align_of::<u16>());
    println!("u32: size={}, alignment={}", size_of::<u32>(), align_of::<u32>());

    // The chapter's numerical diagrams assume these target alignments.
    assert_eq!(align_of::<u16>(), 2, "target differs from chapter assumptions");
    assert_eq!(align_of::<u32>(), 4, "target differs from chapter assumptions");

    println!("Record: size={}, alignment={}, offsets={},{},{}",
        size_of::<Record>(), align_of::<Record>(),
        offset_of!(Record, tag), offset_of!(Record, count), offset_of!(Record, code));
    assert_eq!(size_of::<Record>(), 12);
    assert_eq!(align_of::<Record>(), 4);
    assert_eq!(offset_of!(Record, tag), 0);
    assert_eq!(offset_of!(Record, count), 4);
    assert_eq!(offset_of!(Record, code), 8);
    assert_eq!(size_of::<[Record; 3]>(), 36);

    println!("CompactRecord: size={}, alignment={}",
        size_of::<CompactRecord>(), align_of::<CompactRecord>());
    assert_eq!(size_of::<CompactRecord>(), 8);
    assert_eq!(offset_of!(CompactRecord, code), 4);
    assert_eq!(offset_of!(CompactRecord, tag), 6);

    println!("RustRecord (observed, not guaranteed): size={}, offsets={},{},{}",
        size_of::<RustRecord>(), offset_of!(RustRecord, tag),
        offset_of!(RustRecord, count), offset_of!(RustRecord, code));

    assert_eq!(size_of::<PackedRecord>(), 7);
    assert_eq!(align_of::<PackedRecord>(), 1);
    let packed = PackedRecord { tag: 1, count: 42, code: 7 };
    let count = packed.count; // Copy the u32 value; do not borrow the field.
    println!("Copied packed count: {count}");

    // Raw address without an intermediate reference to the packed field.
    let ptr = std::ptr::addr_of!(packed.count);
    // SAFETY: ptr points to the initialized u32 field in the live packed value.
    // read_unaligned does not require u32 alignment, and u32 is Copy.
    let raw_count = unsafe { ptr.read_unaligned() };
    assert_eq!(raw_count, count);

    assert_eq!(size_of::<AlignedByte>(), 16);
    assert_eq!(align_of::<AlignedByte>(), 16);
    assert_eq!(size_of::<Inner>(), 8);
    assert_eq!(offset_of!(Outer, next), 8);
    assert_eq!(size_of::<Outer>(), 12);

    // Explicit wire format: 1 tag byte, 4 little-endian count bytes, 2 code bytes.
    let bytes = [0x01, 0x78, 0x56, 0x34, 0x12, 0xBC, 0x9A];
    let tag = bytes[0];
    let count = u32::from_le_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]);
    let code = u16::from_le_bytes([bytes[5], bytes[6]]);
    assert_eq!(tag, 1);
    assert_eq!(count, 0x1234_5678);
    assert_eq!(code, 0x9ABC);
    println!("Decoded: tag={tag}, count={count:#X}, code={code:#X}");
    println!("All layout checks passed.");
}
