# Memory Alignment and Padding: A Beginner’s Chapter in Rust

A struct’s size can be larger than the combined sizes of its fields.

For example, fields occupying 1, 4, and 2 bytes might require a **12-byte struct**, even though:

```text
1 + 4 + 2 = 7 bytes
```

The extra bytes are called **padding**. They help satisfy rules called **alignment requirements**.

This chapter explains those rules using byte diagrams, worked examples, and Rust code. You do not need to understand pointers or write `unsafe` code to follow the main examples.

A complete [runnable Rust lab](alignment_lab.rs) accompanies the chapter. I compiled and ran it successfully.

---

**1. Start with memory as a row of numbered boxes**

Imagine memory as a long row of boxes. Each box holds **one byte**, which consists of eight bits.

Each byte has a number identifying its location. That number is its **address**.

```text
Address:   1000   1001   1002   1003   1004   1005
         +------+------+------+------+------+------+
Memory:  | byte | byte | byte | byte | byte | byte |
         +------+------+------+------+------+------+
```

These are illustrative addresses. Real programs receive addresses chosen by the operating system and runtime.

A value can occupy several consecutive bytes:

```text
A u8 occupies 1 byte:

         +------+
         |  u8  |
         +------+

A u32 occupies 4 bytes:

         +------+------+------+------+
         |      one u32 value        |
         +------+------+------+------+
```

In Rust, `u8` means an unsigned 8-bit integer; `u32` means an unsigned 32-bit integer.

Here are some useful sizes:

| Rust type | Size in bytes |
|---|---:|
| `u8` | 1 |
| `u16` | 2 |
| `u32` | 4 |
| `u64` | 8 |
| `[u8; 4]` | 4 |
| `()` | 0 |

`[u8; 4]` is an array containing four `u8` values. `()` is Rust’s unit type, which needs no storage for its value. These sizes are documented by [`size_of`](https://doc.rust-lang.org/std/mem/fn.size_of.html).

**Size answers: “How much space does this value occupy?”**

Alignment answers a different question.

---

**2. Alignment controls where a value may begin**

Suppose a type has an alignment requirement of **4 bytes**.

Its starting address must be divisible by 4:

```text
Address:   1000  1001  1002  1003  1004  1005  1006  1007  1008
             ↑                       ↑                       ↑
          allowed                 allowed                 allowed
```

For example:

```text
1000 ÷ 4 = 250       → aligned
1001 ÷ 4 = 250.25    → misaligned
1004 ÷ 4 = 251       → aligned
```

Only the **starting address** must meet the requirement. The individual bytes of a four-byte value naturally occupy consecutive addresses:

```text
Address:   1000   1001   1002   1003
         +------+------+------+------+
         |      one u32 value        |
         +------+------+------+------+
            ↑
      aligned starting address
```

An alignment of 1 permits any byte address. An alignment of 2 permits multiples of 2, and so on. Rust alignments are powers of two. See the [Rust Reference’s alignment rules](https://doc.rust-lang.org/reference/type-layout.html#size-and-alignment).

For the numerical examples in this chapter, assume:

| Type | Size | Assumed alignment |
|---|---:|---:|
| `u8` | 1 | 1 |
| `u16` | 2 | 2 |
| `u32` | 4 | 4 |

These alignments are common, but alignment is target dependent. Measure it rather than assuming that size and alignment are always equal. The [`align_of` documentation](https://doc.rust-lang.org/std/mem/fn.align_of.html) explicitly notes this distinction.

---

**3. Why do alignment requirements exist?**

Computer hardware often accesses memory in units larger than one byte. The position of a value can affect how the processor retrieves it.

As a simplified illustration, imagine four-byte groups:

```text
Group A                     Group B
+-----+-----+-----+-----+   +-----+-----+-----+-----+
|  0  |  1  |  2  |  3  |   |  4  |  5  |  6  |  7  |
+-----+-----+-----+-----+   +-----+-----+-----+-----+

Aligned four-byte value:
[=======================]

Four-byte value starting at byte 1:
      [==========================]
```

The second value crosses a group boundary.

Actual hardware is more complicated than this picture. Unaligned access can work efficiently on some processors, require extra work in some circumstances, or be restricted for particular instructions.

For Rust programmers, the key point is that **the language has alignment rules regardless of whether a particular processor can execute an unaligned instruction**.

Ordinary references must meet those rules. An `unsafe` block does not switch them off. See Rust’s rules for [misaligned access and invalid references](https://doc.rust-lang.org/reference/behavior-considered-undefined.html).

---

**4. Ask Rust to report size and alignment**

Run this small program:

```rust
use std::mem::{align_of, size_of};

fn main() {
    println!(
        "u8: size={}, alignment={}",
        size_of::<u8>(),
        align_of::<u8>()
    );

    println!(
        "u16: size={}, alignment={}",
        size_of::<u16>(),
        align_of::<u16>()
    );

    println!(
        "u32: size={}, alignment={}",
        size_of::<u32>(),
        align_of::<u32>()
    );
}
```

On the machine used to check this chapter, it prints:

```text
u8: size=1, alignment=1
u16: size=2, alignment=2
u32: size=4, alignment=4
```

The syntax `::<u32>` supplies a type to a generic function. Read `size_of::<u32>()` as “the size of the type `u32`.”

Both functions report byte counts. They inspect a **type**, so you do not need to construct a value first.

---

**5. Padding appears when fields cannot fit immediately together**

Consider this struct:

```rust
#[repr(C)]
struct Record {
    tag: u8,
    count: u32,
    code: u16,
}
```

A **struct** groups named values called **fields**.

The attribute `#[repr(C)]` makes Rust lay out these fields in declaration order using the target’s C layout rules. This gives our diagrams a predictable order.

We will discuss ordinary Rust structs shortly.

First, add the field sizes:

```text
tag:      1 byte
count:    4 bytes
code:     2 bytes
          -------
          7 bytes of field data
```

You might expect this layout:

```text
Offset:   0    1    2    3    4    5    6
        +----+----+----+----+----+----+----+
        |tag |       count      |  code   |
        +----+----+----+----+----+----+----+
```

An **offset** measures distance from the beginning of the struct.

If the struct begins at address 1000, offset 1 means address 1001:

```text
field address = struct starting address + field offset
```

Our proposed layout places `count` at offset 1. With a four-byte-aligned struct base, that does not satisfy `count`’s four-byte alignment.

The compiler therefore leaves a gap before `count`:

```text
Offset:   0    1    2    3    4    5    6    7    8    9
        +----+----+----+----+----+----+----+----+----+----+
        |tag | P  | P  | P  |       count      |  code   |
        +----+----+----+----+----+----+----+----+----+----+

P = padding byte
```

Now:

- `tag` starts at offset 0.
- `count` starts at offset 4.
- `code` starts at offset 8.

These gaps are **internal padding**.

We have reached ten bytes, but the struct still needs two more.

---

**6. Tail padding makes arrays work**

For this `#[repr(C)]` struct, the strongest field alignment is 4, so the struct’s alignment is 4.

Its size must be a multiple of that alignment. Ten is not divisible by four; twelve is.

The complete layout is therefore:

```text
Offset:   0    1    2    3    4    5    6    7    8    9   10   11
        +----+----+----+----+----+----+----+----+----+----+----+----+
        |tag | P  | P  | P  |       count      |  code   | P  | P  |
        +----+----+----+----+----+----+----+----+----+----+----+----+

Field data:         7 bytes
Internal padding:   3 bytes
Tail padding:       2 bytes
Total size:        12 bytes
Alignment:          4 bytes
```

The padding at the end is called **tail padding** or **trailing padding**.

To see why it matters, imagine an array of three records:

```text
Array beginning at illustrative address 1000:

1000                    1012                    1024
  ↓                       ↓                       ↓
+-----------------------+-----------------------+-----------------------+
|       Record 0        |       Record 1        |       Record 2        |
|       12 bytes        |       12 bytes        |       12 bytes        |
+-----------------------+-----------------------+-----------------------+
```

Every element starts at an address divisible by four.

If records occupied only ten bytes, their starting addresses would be:

```text
1000, 1010, 1020, ...
        ↑
   not divisible by 4
```

The second record would begin at a misaligned address.

**A type’s size includes the padding needed to place another value of the same type immediately after it.** Consequently, `[Record; 3]` occupies `3 × 12 = 36` bytes. This relationship is specified by [`size_of`](https://doc.rust-lang.org/std/mem/fn.size_of.html).

---

**7. Verify the diagram with Rust**

This is a complete program:

```rust
use std::mem::{align_of, offset_of, size_of};

#[repr(C)]
struct Record {
    tag: u8,
    count: u32,
    code: u16,
}

fn main() {
    println!("size: {}", size_of::<Record>());
    println!("alignment: {}", align_of::<Record>());

    println!("tag offset: {}", offset_of!(Record, tag));
    println!("count offset: {}", offset_of!(Record, count));
    println!("code offset: {}", offset_of!(Record, code));

    println!("array size: {}", size_of::<[Record; 3]>());
}
```

Under our target assumptions:

```text
size: 12
alignment: 4
tag offset: 0
count offset: 4
code offset: 8
array size: 36
```

`offset_of!` reports a field’s byte offset without reading its contents. It has been available in stable Rust since version 1.77. See its [standard library documentation](https://doc.rust-lang.org/std/mem/macro.offset_of.html).

Measuring offsets is preferable to trying to inspect raw padding bytes.

---

**8. Calculate a layout by hand**

For a plain `#[repr(C)]` struct, use this process:

1. Begin at offset 0.
2. Round the offset up to satisfy the next field’s alignment.
3. Place that field.
4. Advance by the field’s size.
5. Repeat for the remaining fields.
6. Round the final size up to the struct’s alignment.

For `Record`:

| Step | Current offset | Action | Next offset |
|---|---:|---|---:|
| Place `tag` | 0 | Already aligned; use 1 byte | 1 |
| Align `count` | 1 | Skip 3 bytes to reach 4 | 4 |
| Place `count` | 4 | Use 4 bytes | 8 |
| Place `code` | 8 | Already aligned; use 2 bytes | 10 |
| Finish struct | 10 | Skip 2 bytes to reach 12 | 12 |

A useful arithmetic rule is:

```text
padding = (alignment - (offset % alignment)) % alignment
```

Here `%` means “remainder after division.”

For `count`:

```text
offset = 1
alignment = 4

padding = (4 - (1 % 4)) % 4
        = (4 - 1) % 4
        = 3
```

If the offset is already aligned, the result is zero.

These steps follow the [Rust Reference’s `repr(C)` struct layout algorithm](https://doc.rust-lang.org/reference/type-layout.html#reprc-structs).

---

**9. Changing field order can reduce padding**

Try grouping the more strongly aligned fields first:

```rust
#[repr(C)]
struct CompactRecord {
    count: u32,
    code: u16,
    tag: u8,
}
```

The layout becomes:

```text
Offset:   0    1    2    3    4    5    6    7
        +----+----+----+----+----+----+----+----+
        |       count      |  code   |tag | P  |
        +----+----+----+----+----+----+----+----+

Field data:   7 bytes
Padding:      1 byte
Total size:   8 bytes
Alignment:    4 bytes
```

The fields still contain exactly the same kinds of data. Their placement changed.

| Layout | Field bytes | Padding bytes | Total bytes |
|---|---:|---:|---:|
| `Record` | 7 | 5 | 12 |
| `CompactRecord` | 7 | 1 | 8 |

For one million elements, that is a reduction from 12,000,000 to 8,000,000 bytes of element storage.

For simple `repr(C)` structs, placing fields in descending alignment order is a useful starting point. It is a layout technique, not a universal performance guarantee. The standard library provides a similar [field reordering example](https://doc.rust-lang.org/std/mem/fn.size_of.html#examples).

Changing field order also changes offsets. If another program expects a particular layout, those offsets are part of the interface and cannot be changed casually.

---

**10. Ordinary Rust structs may have a different field order**

Without `#[repr(C)]`, this struct uses Rust’s default representation:

```rust
struct RustRecord {
    tag: u8,
    count: u32,
    code: u16,
}
```

The compiler may place its fields in a different memory order from their declaration order.

In the lab run accompanying this chapter, the observed result was:

```text
size:          8 bytes
tag offset:    6
count offset:  0
code offset:   4
```

That corresponds to:

```text
Offset:   0    1    2    3    4    5    6    7
        +----+----+----+----+----+----+----+----+
        |       count      |  code   |tag | P  |
        +----+----+----+----+----+----+----+----+
```

This is an observation from that compilation, not a promise for every compiler or target.

Default Rust representation allows layout flexibility, including field reordering. It does not guarantee the smallest possible layout. The [Rustonomicon explains this freedom](https://doc.rust-lang.org/nomicon/repr-rust.html).

Use ordinary structs for ordinary application data. Choose a representation attribute when you have a concrete layout requirement.

---

**11. Nested structs bring their padding with them**

Consider these definitions:

```rust
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
```

Under our assumptions, `Inner` occupies eight bytes:

```text
Inner:
Offset:   0    1    2    3    4    5    6    7
        +----+----+----+----+----+----+----+----+
        |       number     |flag| P  | P  | P  |
        +----+----+----+----+----+----+----+----+
```

When `Inner` becomes a field of `Outer`, it still occupies eight bytes. `next` begins after that entire field:

```text
Outer:
Offset:   0    1    2    3    4    5    6    7    8    9   10   11
        +----+----+----+----+----+----+----+----+----+----+----+----+
        |       number     |flag| P  | P  | P  |next| P  | P  | P  |
        +----+----+----+----+----+----+----+----+----+----+----+----+
        <-------------- inner -------------->

Outer size: 12 bytes
```

The outer struct does not put `next` into the inner struct’s tail padding.

Also, applying `repr(C)` to an outer type does not change the representation of its nested field types. That is why both definitions above have the attribute. See the [rules for representation and field layout](https://doc.rust-lang.org/reference/type-layout.html#representations).

---

**12. What happens if we remove the padding?**

Rust offers packed representations:

```rust
#[repr(C, packed)]
struct PackedRecord {
    tag: u8,
    count: u32,
    code: u16,
}
```

For these primitive fields:

```text
Offset:   0    1    2    3    4    5    6
        +----+----+----+----+----+----+----+
        |tag |       count      |  code   |
        +----+----+----+----+----+----+----+

Size:       7 bytes
Alignment:  1 byte
```

The tradeoff is that `count` may sit at an address unsuitable for an ordinary `&u32` reference.

Copying the field into a local variable is allowed:

```rust
let packed = PackedRecord {
    tag: 1,
    count: 42,
    code: 7,
};

let count = packed.count;
println!("{count}");
```

`u32` is a `Copy` type, so this reads a value into a separate local.

Borrowing that packed field is rejected:

```rust
// Does not compile:
let reference = &packed.count;
```

A formatting macro can borrow its argument too:

```rust
// Also rejected:
println!("{}", packed.count);
```

The local-copy version avoids creating a reference to the packed field. Rust documents these cases under [compiler error E0793](https://doc.rust-lang.org/error_codes/E0793.html).

**Treat `packed` as a tool for specific layouts, rather than a routine way to save memory.**

---

**13. Optional advanced note: deliberate unaligned reads**

You can skip this section on a first reading.

Low-level code sometimes needs to read from an unaligned location. Rust provides a dedicated operation:

```rust
let ptr = std::ptr::addr_of!(packed.count);

// SAFETY: ptr points to the initialized u32 field in the live
// packed value. This operation permits an unaligned address.
let count = unsafe { ptr.read_unaligned() };
```

Two details matter:

- `addr_of!` obtains a raw pointer without first creating a reference to the field.
- `read_unaligned()` permits an unaligned address.

It still requires valid, readable memory containing a properly initialized value. Removing the alignment requirement does not remove all other requirements.

For this example, `u32` is also `Copy`, so copying its value does not create ownership problems. See [`read_unaligned`](https://doc.rust-lang.org/std/ptr/fn.read_unaligned.html).

---

**14. Alignment can also be increased**

This type explicitly requests a 16-byte alignment:

```rust
#[repr(C, align(16))]
struct AlignedByte(u8);
```

Its one-byte field is followed by padding:

```text
Offset:  0   1   2   3   4   5   6   7   8   9  10  11  12  13  14  15
       +---+---+---+---+---+---+---+---+---+---+---+---+---+---+---+---+
       | u8| P | P | P | P | P | P | P | P | P | P | P | P | P | P | P |
       +---+---+---+---+---+---+---+---+---+---+---+---+---+---+---+---+

Size:      16 bytes
Alignment: 16 bytes
```

The larger size allows consecutive array elements to remain 16-byte aligned.

Notice that this does not turn the `u8` field into a 16-byte integer. The field still holds one byte of data; the containing type has stronger placement requirements.

The [`align` and `packed` modifiers](https://doc.rust-lang.org/reference/type-layout.html#the-alignment-modifiers) adjust layout requirements in opposite directions.

---

**15. Padding is not application data**

A diagram labels padding with `P` so you can see where it occurs. That does **not** mean the program stores a particular byte value there.

Initializing every field does not guarantee initialized padding. Do not assume padding contains zero, inspect it as ordinary initialized bytes, or use it to hold hidden information.

This distinction also explains why dumping a struct’s memory is a poor general-purpose file format: padding may be uninitialized, and reading uninitialized integer bytes violates Rust’s validity rules. See [Rust’s rules for valid values](https://doc.rust-lang.org/reference/behavior-considered-undefined.html#invalid-values).

A data format should specify the bytes it actually needs.

For example, define a seven-byte record format explicitly:

```text
Byte offset:  0       1    2    3    4       5    6
            +-----+---------------------+-------------+
            | tag | count, little-endian| code, LE    |
            +-----+---------------------+-------------+
```

Then decode those bytes safely:

```rust
fn main() {
    let bytes = [
        0x01,                   // tag
        0x78, 0x56, 0x34, 0x12, // count
        0xBC, 0x9A,             // code
    ];

    let tag = bytes[0];

    let count = u32::from_le_bytes([
        bytes[1], bytes[2], bytes[3], bytes[4],
    ]);

    let code = u16::from_le_bytes([
        bytes[5], bytes[6],
    ]);

    println!("tag = {tag}");
    println!("count = {count:#X}");
    println!("code = {code:#X}");
}
```

Output:

```text
tag = 1
count = 0x12345678
code = 0x9ABC
```

“Little-endian” means the least significant byte comes first. The [`from_le_bytes` conversion](https://doc.rust-lang.org/std/primitive.u32.html#method.from_le_bytes) explicitly interprets that order.

**Alignment concerns where a value begins. Endianness concerns how its bytes are ordered.**

This decoding approach needs no cast, packed struct, or `unsafe` block.

---

**16. Check your understanding**

For these exercises, continue assuming `u16` alignment is 2 and `u32` alignment is 4.

**Exercise A: Predict the size.**

```rust
#[repr(C)]
struct A {
    first: u8,
    second: u16,
    third: u8,
}
```

Work out each field offset before calculating the final size.

**Answer:**

```text
Offset:   0      1      2      3      4      5
        +------+------+------+------+------+------+
        |first |  P   |    second   |third |  P   |
        +------+------+------+------+------+------+

Offsets:   first=0, second=2, third=4
Alignment: 2
Size:      6
```

**Exercise B: Reduce the size by reordering fields.**

One solution is:

```rust
#[repr(C)]
struct B {
    second: u16,
    first: u8,
    third: u8,
}
```

```text
Offset:   0      1      2      3
        +------+------+------+------+
        |    second   |first |third |
        +------+------+------+------+

Alignment: 2
Size:      4
Padding:   0
```

**Exercise C: How large is `[A; 10]`?**

It occupies **60 bytes**:

```text
10 elements × 6 bytes per element = 60 bytes
```

Each element includes its padding.

**Exercise D: Does a four-byte size imply four-byte alignment?**

No. `[u8; 4]` has size 4 but alignment 1. It contains four individually byte-aligned elements. Rust specifies that an array has the alignment of its element type. See [array layout](https://doc.rust-lang.org/reference/type-layout.html#array-layout).

---

**17. A practical reference**

| Question | Rust tool or rule |
|---|---|
| How many bytes does a type occupy? | `size_of::<T>()` |
| What alignment does it require? | `align_of::<T>()` |
| Where does a struct field begin? | `offset_of!(T, field)` |
| Do fields follow declaration order? | For `repr(C)` structs; default Rust does not promise this |
| Why is there padding between fields? | To satisfy the next field’s alignment |
| Why is there padding at the end? | To make the type’s size a multiple of its alignment |
| Can packing complicate access? | Yes; fields may be unsuitable for ordinary references |
| How should external bytes be interpreted? | Decode an explicit format with conversions such as `from_le_bytes` |

