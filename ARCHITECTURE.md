# ndata Architecture

This document describes how ndata works internally. For usage, see the
[README](README.md).

## Overview

At runtime, ndata maintains three global heaps:

| Heap | Element type | Handle type | Module |
|---|---|---|---|
| `OBJECT_HEAP` | `HashMap<String, Data>` | `DataObject` | `src/dataobject.rs` |
| `ARRAY_HEAP` | `Vec<Data>` | `DataArray` | `src/dataarray.rs` |
| `BH` (bytes heap) | `DataStream` (bytes + read/write state + MIME type) | `DataBytes` | `src/databytes.rs` |

Each heap is a `static` guarded by a `SharedMutex`, paired with a second
static — a drop queue (`Vec<usize>`) — that records handles whose `Drop` has
run but whose reference count has not yet been reconciled.

The public types are thin handles: each is a struct holding a single
`data_ref: usize`, the index of its data in the corresponding heap. All actual
data is owned by the heaps. This is how ndata sidesteps the borrow checker:
from Rust's perspective you only ever hold a small index-carrying struct, and
safety is enforced at runtime by locks and reference counts instead of at
compile time by ownership.

The `Data` enum ties the types together. Scalars (`DString`, `DInt`, `DFloat`,
`DBoolean`, `DNull`) carry their values inline; the container variants
(`DObject(usize)`, `DArray(usize)`, `DBytes(usize)`) carry a heap index.
Storing `Data::DObject(child_ref)` inside a parent's map is what creates a
reference graph — including, if you choose, cycles.

## Heap and slot allocation

`Heap<T>` (`src/heap.rs`) is a reference-counted object pool. Each entry is a
`Blob<T>` holding the value and its count. Its operations:

* `push(value)` — allocate with a count of 1, returning a `usize` key
* `incr(key)` / `decr(key)` — adjust the count; `decr` **removes the entry
  immediately** when the count would reach zero
* `get(key)` (panicking), `try_get(key)` (checked), `contains_key`, `count`

`Heap` stores entries in a `UsizeMap<T>` (`src/usizemap.rs`): a
`Vec<Option<T>>` plus a free-list of vacated indices. Removal pushes the index
onto the free-list, and the next insertion **reuses** it. Index reuse keeps
the storage dense, but it has an important consequence: a stale reference to a
freed slot doesn't point at nothing — it may point at *someone else's newer
data*. This is why application code must never call `incr()`/`decr()` by hand;
an unbalanced `decr` frees a slot other handles still reference, and the
eventual reconciliation of those handles can then corrupt the count of an
unrelated allocation that reused the slot.

## Reference-counting lifecycle

Counts change at these points:

* **Creation** — `DataObject::new()` etc. push into the heap: count = 1.
* **Cloning a handle** — `clone()` increments immediately.
* **`DataObject::get(ref)` / `DataArray::get(ref)` / `DataBytes::get(ref)`** —
  construct a handle from a raw index, incrementing immediately.
* **Insertion** — `set_property` / `push_property` (and the typed `put_*` /
  `push_*` wrappers) increment the count of any container value being
  inserted, and release any container value being replaced.
* **Dropping a handle** — the `Drop` impl does *not* decrement. It pushes the
  `data_ref` onto the type's drop queue. The decrement is deferred.

`ndata::gc()` reconciles: for each queued reference it calls the type's
internal `delete()`, which decrements the count — and when a count reaches
zero (the entry is about to be freed), first walks the container's contents
and recursively deletes any child objects and arrays it referenced. Deferred
counting means normal operation never pays for freeing; cost is concentrated
in the explicit collection call, and nothing is ever freed while any live
handle (or containing structure) still references it.

A worked example — `parent.put_object("k", child)`:

1. `child` was created with count 1.
2. `put_object` moves the handle in and stores `Data::DObject(child.data_ref)`
   in the parent's map, incrementing the count to 2 (the map's reference).
3. The moved-in handle drops at the end of the call, queueing one decrement.
4. After the next `gc()`, the count settles at 1 — held by the parent, exactly
   as intended.

The same accounting makes error handling composable: code that builds
temporary structures (the JSON parser, for instance) can simply drop its
handles on any failure path, and `gc()` reclaims the entire partial tree.

## Locking: SharedMutex

`SharedMutex<T>` (`src/sharedmutex.rs`) is a custom spin-lock wrapper built
for two requirements ordinary `std` primitives don't meet here: it can live in
a `static` and be initialized explicitly at runtime (`ndata::init()`), and it
can be re-pointed at memory owned by another copy of the library (see
mirroring below). Internally it keeps a raw pointer to an atomic lock word and
a raw pointer to the data cell.

Locking is coarse: one mutex per heap. Every operation — a getter, a setter, a
clone — takes the lock for its heap, does its work, and releases. Two threads
can't race on the same object, or even on different objects in the same heap;
the design trades parallelism for simplicity and makes every handle `Send +
Sync` for free. Operations hold at most one heap lock at a time (with a
consistent oheap → aheap ordering in `gc()`), which keeps the design
deadlock-free.

Forgetting `ndata::init()` means the statics are uninitialized, and the first
`lock()` panics with an explicit message rather than misbehaving.

## Mirroring (hot reload)

`ndata::init()` returns an `NDataConfig` containing, for each heap and drop
queue, the raw addresses of its lock word and data cell (obtained via
`SharedMutex::share()`). `NDataConfig` can round-trip through a hex string
(`to_string()` / `from_string()`) for easy transport across an FFI boundary.

A dynamic library loaded into the same process calls `ndata::mirror(config)`
instead of `init()`. Each of its own `SharedMutex` statics is initialized in
"mirrored" state: its pointers are aimed at the host's lock word and data.
From then on, both copies of the library operate on one set of heaps under one
set of locks — which is what lets
[hot-lib-reloader](https://crates.io/crates/hot-lib-reloader)-style code
swapping preserve all runtime state.

Because the mechanism is literally shared *addresses*, it only works inside a
single process's address space. Passing an `NDataConfig` to a separate process
and mirroring there cannot work — the `examples/multiprocess` example
demonstrates the resulting crash on purpose.

## JSON (json_util)

When the `serde_support` feature is off, `src/json_util.rs` provides the JSON
implementation behind `to_string()`/`from_string()`. Design points:

* **Iterative, not recursive.** Both the parser and serializer maintain an
  explicit stack, so nesting depth is bounded by available memory, not the
  call stack.
* **Strict parsing** per RFC 8259: JSON's exact whitespace set, no trailing
  commas, no leading zeros, no unescaped control characters, full
  `\uXXXX`/surrogate-pair handling, typed `ParseError`s, and no panics on any
  input.
* **No manual reference counting.** The parser builds containers with the same
  public API as user code; on error it just drops its handles and the next
  `gc()` reclaims the partial tree.
* **Total serialization.** The serializer takes per-node snapshots under the
  heap lock, detects reference cycles via its frame stack (emitting `null` at
  the point of the cycle), renders dangling references and non-finite floats
  as `null`, writes keys in sorted order, and keeps floats type-stable
  (`1.0`, not `1`).

With `serde_support` on, the same public methods route through `serde_json`,
and `from_json()`/`to_json()` expose direct `serde_json::Value` interop. The
serialization behaviors above (sorted keys, hex-encoded `DataBytes`) are
aligned between the two paths.

## Design summary

ndata is a singleton memory-pool design with deferred reference-counting
garbage collection. Its guiding trade-offs:

* **Runtime checks over compile-time checks** — handles and heaps replace
  ownership and borrowing, so dynamic, shared, and cyclic structures are easy;
  in exchange, mistakes surface at runtime.
* **Coarse locks over fine-grained concurrency** — one lock per heap makes the
  model simple and race-free at the cost of parallel throughput.
* **Deferred over immediate reclamation** — dropping is cheap and freeing is
  batched, at the cost of remembering to call `gc()`.
