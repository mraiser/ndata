# ndata

**Self-owned, JSON-like data structures for Rust.**

ndata provides thread-safe, dynamically typed data structures — objects, arrays,
strings, numbers, booleans, byte streams, and null — that live in an internal
heap with manual garbage collection. You can create, nest, clone, and share them
freely, across threads and scopes, without fighting the borrow checker: the
library manages ownership and cleanup internally, and you call `ndata::gc()`
when you want memory reclaimed.

This is deliberately *not* idiomatic Rust. It trades some performance and
compile-time guarantees for the convenience of a garbage-collected language,
which makes it a good fit for rapid prototyping, global state, dynamic
configuration, and hot-reload scenarios — and a poor fit for hot inner loops.
The included examples show how to use it; once your logic is worked out, the
dynamic parts are easy to refactor into plain Rust.

**Lightweight and self-contained:** ndata has no third-party dependencies by
default. Integration with `serde_json` is opt-in via a feature flag.

## Use cases

* **Rapid prototyping** — build nested, dynamic structures without declaring
  types up front, like working with `serde_json::Value` or a scripting
  language's objects.
* **Multithreaded sharing** — every handle is `Send + Sync`; internal locking
  means no `Arc<Mutex<...>>` wrapping, ever.
* **Global state** — put data in ndata's heap and reach it from anywhere,
  without `static mut`, `lazy_static`, or unsafe code.
* **Panic recovery** — data lives in an independent heap, so it survives a
  thread's panic and can be inspected while unwinding or afterward.
* **Hot reloading** — with the [hot-lib-reloader](https://crates.io/crates/hot-lib-reloader)
  crate, newly loaded code can attach to the running program's ndata heaps and
  pick up all existing state (see [Hot reloading](#hot-reloading-mirror) below).

## Installation

```toml
[dependencies]
ndata = "0.3"
```

To route JSON handling through serde instead of the built-in parser:

```toml
[dependencies]
ndata = { version = "0.3", features = ["serde_support"] }
```

## Quickstart

Call `ndata::init()` once at startup, before touching any ndata type. (Skipping
it makes the first heap access panic. Nothing is read from or written to disk —
the heaps are purely in-memory.)

```rust
use ndata::dataobject::DataObject;
use ndata::dataarray::DataArray;

fn main() {
  ndata::init();

  // Build a dynamic object -- no type declarations, no lifetimes.
  let mut obj = DataObject::new();
  obj.put_string("name", "Alice");
  obj.put_int("age", 30);

  let mut hobbies = DataArray::new();
  hobbies.push_string("reading");
  hobbies.push_string("hiking");
  obj.put_array("hobbies", hobbies);

  // Clones are cheap handles to the *same* data.
  let mut same = obj.clone();
  same.put_boolean("active", true);
  assert!(obj.get_boolean("active"));

  // JSON round-trip. Keys serialize in sorted order.
  let json = obj.to_string();
  println!("{}", json);
  let parsed = DataObject::from_string(&json);
  assert_eq!(parsed.get_int("age"), 30);

  // Reclaim memory for dropped values whenever it suits you.
  drop(obj);
  drop(same);
  drop(parsed);
  ndata::gc();
}
```

Sharing across threads needs no synchronization on your side:

```rust
use ndata::dataobject::DataObject;

fn main() {
  ndata::init();
  let obj = DataObject::new();

  let mut handle = obj.clone();
  std::thread::spawn(move || {
    handle.put_string("greeting", "hello from a thread");
  })
  .join()
  .unwrap();

  println!("{}", obj.get_string("greeting"));
  ndata::gc();
}
```

## Core types

| Type | What it is | Backing storage |
|---|---|---|
| `Data` | Enum holding any value: `DObject(usize)`, `DArray(usize)`, `DBytes(usize)`, `DString(String)`, `DInt(i64)`, `DFloat(f64)`, `DBoolean(bool)`, `DNull` | Scalars inline; containers by heap index |
| `DataObject` | Dynamic map with `String` keys, kept in sorted key order | `BTreeMap<String, Data>` in the object heap |
| `DataArray` | Dynamic list | `Vec<Data>` in the array heap |
| `DataBytes` | Byte *stream* — bytes plus read/write-open flags and an optional MIME type | `DataStream` in the bytes heap |

`DataObject`, `DataArray`, and `DataBytes` are lightweight handles (a single
`data_ref: usize` index into a global heap). Cloning a handle clones the
*reference*, not the data; use `deep_copy()` for a true duplicate, or
`shallow_copy()` for a new container that shares its nested children.

## Memory management

ndata uses deferred reference counting:

* Creating a value allocates it in a global heap with a count of 1. Cloning a
  handle, or inserting a container into another container, increments the
  count. Mutators (`put_*`, `push_*`, `set_property`) handle this for you,
  including releasing values they replace.
* Dropping a handle does **not** free anything — it queues the reference for
  the next collection.
* `ndata::gc()` processes the queues: counts are decremented, and any value
  that reaches zero is freed, recursively releasing its children.

The rules that follow from this design:

* **Call `ndata::gc()` periodically** (or at natural checkpoints). Nothing is
  ever reclaimed without it, so a long-running program that never calls it
  leaks by design.
* **Never call `incr()`/`decr()` yourself.** The handles and mutators already
  balance every count, and heap slots are reused after a value is freed — a
  stray manual `decr()` can free a value whose handles are still live and
  later corrupt an unrelated allocation. The methods exist for FFI-style edge
  cases; ordinary code (including code that nests containers or moves them
  between threads) never needs them.
* Moving a container into a parent consumes the handle:
  `parent.put_object("k", child)` takes `child` by value. Clone first —
  `parent.put_object("k", child.clone())` — if you need to keep using it.
* `DataObject::get_keys()` borrows the handle; the older `keys()` consumes it.

## Thread safety and locking

Each heap (objects, arrays, bytes) is guarded by one global lock, and every
operation acquires the lock it needs for just that operation. This coarse
locking is what makes the "share anything anywhere" model safe — and it means
ndata serializes access: only one thread at a time can be touching the object
heap, regardless of *which* object it's touching. For coordination-heavy
workloads that's fine; don't put ndata operations in a tight loop you need to
run in parallel.

Because handles are already internally synchronized, wrapping them in `Arc`,
`Rc`, `Mutex`, or `RwLock` is never necessary and only adds overhead and
deadlock potential.

## Error handling

Every typed getter comes in two flavors:

```rust
// Panicking -- convenient for prototypes:
let name = obj.get_string("name");

// Non-panicking -- returns Result<_, NDataError>:
match obj.try_get_string("name") {
  Ok(name) => println!("{}", name),
  Err(e) => eprintln!("{}", e), // KeyNotFound or WrongDataType
}
```

The `try_get_*` family exists on `DataObject`, `DataArray`, and `DataBytes`.
Prefer it in anything beyond a prototype. For parsing,
`DataObject::try_from_string` returns a `Result` instead of panicking on
malformed JSON, as do `json_util::object_from_string` /
`json_util::array_from_string`.

## JSON

`DataObject::to_string()` / `from_string()` and their `DataArray` equivalents
convert to and from JSON text. Two interchangeable implementations sit behind
them:

* **Default:** a built-in, dependency-free parser and serializer
  (`ndata::json_util`). Parsing is strict RFC 8259 — it rejects trailing
  commas, leading zeros, unescaped control characters, and trailing input,
  with typed `ParseError`s. Both the parser and serializer are iterative, so
  deeply nested data can't overflow the stack.
* **With `serde_support`:** conversion routes through `serde_json`, and
  `from_json()` / `to_json()` interop directly with `serde_json::Value`.

Behavior is aligned between the two paths:

* Object keys serialize in **sorted order** (deterministic output).
* Non-finite floats (`NaN`, `±inf`) serialize as `null`.
* Finite floats always round-trip as floats (`1.0` serializes as `1.0`, not `1`).
* A reference **cycle** (ndata lets containers contain themselves) serializes
  as `null` at the point of the cycle rather than recursing forever.
* `DataBytes` serializes as its hex string (e.g. `"48 65 6C 6C 6F"`). This is
  one-way: parsing it back yields a `DString`, not a `DataBytes`.

## Hot reloading (mirror)

ndata's heaps can be shared across dynamic-library boundaries **within a single
process**. `ndata::init()` returns an `NDataConfig` describing the heaps; a
freshly loaded library passes it to `ndata::mirror(config)` to attach to the
running program's data instead of creating its own:

```rust
// In the host (call once at startup):
let config = ndata::init();

// In a newly loaded dynamic library (call once, instead of init):
ndata::mirror(config);
// ... all existing DataObjects/DataArrays are now visible here.
```

Combined with [hot-lib-reloader](https://crates.io/crates/hot-lib-reloader),
this lets you swap in freshly compiled code while every runtime variable
survives — see the [hot-reload example](examples/hot-reload/).

The mechanism shares raw in-process memory addresses, so it does **not** work
across separate processes. The [multiprocess example](examples/multiprocess/)
exists precisely to demonstrate that limitation.

## no_std

ndata works in `#![no_std]` environments with an allocator. Disable the
default `std` feature:

```toml
[dependencies]
ndata = { version = "0.3", default-features = false }
```

What to know:

* **An allocator is required** (this is no_std-with-`alloc`, not bare metal
  without allocation). Provide a `#[global_allocator]` as usual.
* **Atomics with compare-and-swap are required.** The internal spin lock uses
  `AtomicUsize` CAS, so targets without atomic CAS (e.g. `thumbv6m-none-eabi`)
  are not supported.
* **Don't touch ndata from interrupt handlers.** The locks spin; an ISR that
  preempts a lock holder on a single core will spin forever. Use it from
  RTOS tasks/threads.
* Without `std`, the diagnostic warnings some methods print on invalid refs
  are compiled out, and `serde_support` is unavailable (it implies `std`).
* Minimum supported Rust version is 1.81. CI builds the crate for
  `thumbv7em-none-eabihf` to keep no_std support from regressing.

## Examples

Each example is a standalone crate in [`examples/`](examples/) with its own
README. Run one with:

```
cd examples/globals
cargo run
```

| Example | Demonstrates |
|---|---|
| [doublylinkedlist](examples/doublylinkedlist/) | Structures the borrow checker hates — a doubly linked list, with back-references, in dynamic data |
| [multithreaded](examples/multithreaded/) | Sharing and mutating the same data from many threads with no visible locking |
| [globals](examples/globals/) | Global state without `static mut`, wrappers, or unsafe code |
| [panic](examples/panic/) | Recovering data after a thread panics |
| [garbage-collection](examples/garbage-collection/) | Toaster-Simple™ garbage collection: what `gc()` frees, and when |
| [hot-reload](examples/hot-reload/) | Live code swapping with state preserved, via hot-lib-reloader + `mirror()` |
| [multiprocess](examples/multiprocess/) | A negative demonstration: mirroring across *separate processes* is not possible (the child crashes, on purpose) |

## Architecture

The short version: all data lives in three global, lock-guarded heaps (objects,
arrays, bytes); the public types are tiny handles holding an index into those
heaps; reference counts are maintained automatically and reconciled when
`gc()` runs. For the full story — the heap and slot-reuse machinery, the
drop-queue lifecycle, `SharedMutex`, and how `mirror()` works — see
[ARCHITECTURE.md](ARCHITECTURE.md).

## LLM usage

The following is a good system prompt for LLMs writing code against ndata:

```
You are an AI assistant working with the ndata crate in Rust. Key points:

Core: ndata provides globally shared, thread-safe, JSON-like dynamic data structures:
- Data: enum (DObject(usize), DArray(usize), DBytes(usize), DString(String), DInt(i64), DFloat(f64), DBoolean(bool), DNull).
- DataObject: handle to a heap-stored BTreeMap<String, Data> (keys iterate in sorted order).
- DataArray: handle to a heap-stored Vec<Data>.
- DataBytes: handle to a heap-stored byte stream (bytes, read/write-open flags, optional MIME type).
Handles hold a data_ref: usize (an index into a global in-memory heap).

CRITICAL RULE - NO WRAPPING: DataObject, DataArray, DataBytes, and Data are already reference-counted and thread-safe internally (via SharedMutex). Do NOT wrap them in Arc, Rc, Mutex, RwLock, etc. That causes double-locking and bugs.

Initialization & GC:
- Call ndata::init() exactly once at startup. It takes no arguments and touches no files; it returns an NDataConfig, which is only needed for mirror()/hot-reload scenarios.
- Garbage collection is manual. Dropping a handle only queues its data_ref; memory is reclaimed when ndata::gc() runs. Long-running programs must call gc() periodically.

Handles & reference counting:
- clone() creates another handle to the SAME data and increments its count. DataObject::get(ref) / DataArray::get(ref) / DataBytes::get(ref) do the same from a raw data_ref.
- Mutators (put_*, push_*, set_property) automatically manage the counts of values being inserted or replaced. NEVER call incr()/decr() manually; the framework balances every count.
- deep_copy() duplicates content into a new instance; shallow_copy() makes a new container that shares nested children.

Nesting containers (CRUCIAL PATTERN): parent.put_object("k", child) MOVES child into the method. The parent stores child.data_ref and increments its count; the moved-in handle then drops, queueing the matching decrement. Net effect: the parent now holds the reference. Because child was moved, the variable is gone -- clone first (parent.put_object("k", child.clone())) if you still need it.

Accessors:
- Panicking: obj.get_string("k"), obj.get_object("k"), arr.get_int(0), etc.
- Non-panicking: try_get_* variants return Result<_, NDataError> (KeyNotFound / WrongDataType). Prefer these outside prototypes.
- DataObject::get_keys() borrows; the older keys() consumes the handle.
- DataBytes: get_data() -> Vec<u8>, current_len(), is_read_open(), to_hex_string().

JSON:
- Default (no features): strict RFC 8259 parser/serializer in ndata::json_util. object_from_string/array_from_string return Result<_, ParseError>. DataObject::from_string panics on bad input; try_from_string does not.
- Serialization details (both code paths): object keys are written in sorted order; NaN/infinity serialize as null; reference cycles serialize as null at the cycle point; DataBytes serializes as an uppercase spaced hex string ("48 65 6C 6C 6F") and reparses as a DString, not bytes.
- With feature "serde_support": from_json()/to_json() interop with serde_json::Value, and to_string/from_string route through serde_json.

Infer other method details from these patterns. Focus on correct reference management (how moves and clones interact with counts when nesting), the no-wrap rule, and remembering to call gc().
```

## License

MIT
