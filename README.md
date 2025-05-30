# NData 
**Self-Owned, JSON-like Data Structures for Rust**

NData is a Rust library that provides self-owned, JSON-like data structures with an internal heap and manual garbage collection. It allows developers to use dynamic data types (objects, arrays, strings, numbers, booleans, byte buffers, and null) in Rust with memory management that feels similar to higher-level languages. This project aims to make Rust more convenient for certain scenarios by handling ownership and cleanup internally. In summary, NData’s functionality is to serve as a dynamic data container (like a JSON Value) that you can freely create and share without worrying about Rust’s usual ownership rules, at the cost of manual GC when needed.

**Notably, NData is designed to be lightweight and portable:**

* It has no third-party crate dependencies by default. All integrations with external crates (like Serde for JSON) are opt-in via feature flags.

* It supports #![no_std] environments (with an allocator), making it suitable for embedded development.

## Key problems and use cases NData addresses include:

* Rapid prototyping: NData brings some flexibility of garbage-collected languages into Rust. You can quickly build complex data structures (e.g., nested objects/arrays) without strict compile-time types. This lowers upfront development effort for prototyping, albeit with some runtime overhead. (Note that this convenience “does re-introduce performance penalties and potential bugs”, which should be refactored out in production code after prototyping.)

* Multithreaded sharing: The library is thread-safe by design, so an NData value (like a DataObject or DataArray) can be shared across threads without manual synchronization or ownership gymnastics. The internal implementation handles locking, so you don’t need to wrap these data in Arc<Mutex> – concurrent threads can read/write the same NData structures safely.

* Global data storage: NData provides an easy way to use global or heap-allocated data in Rust without unsafe code or manual pointers. This can be useful when you truly need global state. Instead of fiddling with static mut or lazy_static, you can put data into NData’s heap and retrieve it from anywhere, letting NData manage the memory.

* Panic safety: Because NData’s structures live in an independent heap with their own lifetime, they are not lost on panic unwinding. This means if a thread panics, you can still recover or inspect data that was stored in NData’s global structures after the panic, which can aid in error handling or debugging.

* Live code reloading (hot-swap): NData is designed to work with runtime code reload scenarios where dynamic libraries are swapped within the same process. In conjunction with the hot-lib-reloader crate, you can swap in new code (a new dynamic library) for a running application while preserving the state of all NData variables. This is achieved by sharing the same internal heap between the old and new code (see the “mirror” feature below), enabling hot-swappable modules without losing in-memory data.

In short, NData acts as a flexible, GC-enabled data store in Rust, useful for quickly building dynamic data (similar to serde_json::Value or a scripting language’s objects) and managing it across threads, global contexts, panics, or even dynamic library reloads within the same process. It achieves this with no default external dependencies and no_std compatibility, trading some performance and fine-grained control for convenience, targeting scenarios like prototypes, REPLs, embedded systems, or long-running services that benefit from dynamic configuration/state.

## Technologies Used

* Implementation Language: NData is implemented entirely in Rust (100% Rust code). It is provided as a Rust crate (library) and uses Rust 2021 edition. By default, it only relies on the Rust standard library (or alloc in no_std mode).

* Core Libraries & Data Structures: The project mainly uses Rust’s standard collections and primitives (or their alloc counterparts). For example, it uses HashMap<String, Data> to represent objects and Vec<Data> for arrays internally. Concurrency is handled via synchronization primitives (see below). NData does not require an external runtime or heavy frameworks and has no third-party dependencies by default – it is a self-contained library. It leverages Rust’s ownership system under the hood (e.g., for reference counting and locking) but abstracts those details away from the user.

* Optional JSON/Serde support: NData can interoperate with JSON. It has an optional dependency on Serde JSON (serde_json) to allow converting between NData types and serde_json::Value for serialization/deserialization. This feature is enabled via the "serde_support" Cargo feature flag and is not active by default. If not using Serde, NData includes its own json_util module for basic JSON operations.

* No-std and portability: The crate declares a "no_std_support" feature, indicating it can be built in a #![no_std] environment using the Rust alloc crate. This makes NData highly portable and suitable for embedded systems or other environments without the full standard library, as long as an allocator is available. By default, on desktop/standard Rust, it uses std for thread locks and collections.

* Hot-swap (mirror) capability: NData provides a feature flag "mirror" which enables sharing the data heap across dynamic library boundaries within the same process. With this enabled, the library can expose the raw memory of its heaps so that a newly loaded dynamic library can attach (“mirror”) to them. This is the mechanism that powers the hot-reload use case (in collaboration with an external reloader). Internally, this involves low-level handling of memory pointers. The "mirror" feature is off by default but can be turned on if you plan to hotswap code or use NData in a multi-library (within the same process) setting.

* Synchronization primitives: Rather than using standard Mutex or RwLock directly, NData defines a custom SharedMutex<T> type. This is essentially a simple mutex wrapper that can be accessed globally (as statics) and, when the mirror feature is on, can work across dynamic libraries (within the same process). Under the hood, it uses the operating system’s synchronization (on supported platforms) for thread-safety. The use of SharedMutex indicates that thread synchronization is a core part of NData’s design, but it’s abstracted so that enabling cross-dynamic-library sharing (within the same process) is possible. The mirror support enables SharedMutex to transform its internal pointer when mirror() is called, pointing it at an existing memory location accessible by different dynamic libraries within the same process.

* Memory management: The library implements its own basic memory management (a heap with manual garbage collection) in pure Rust. It does not use a GC library or external allocator beyond Rust’s default (or alloc in no_std). Reference counting is implemented manually (via atomic counters or indices) rather than using Rc/Arc. This gives fine control over when to free memory (only during explicit gc() calls). The trade-off is that the developer must remember to call ndata::gc() periodically – memory won’t be reclaimed otherwise.

Overall, Rust’s ecosystem is the main technology here – NData is delivered as a crates.io package (current version ~0.3.11) and built with Cargo. It is MIT licensed and integrates with typical Rust tooling (documentation on docs.rs, etc.). It is designed to be lightweight and portable, with no mandatory third-party dependencies and full no_std support, making it adaptable to a wide range of Rust projects, including embedded applications.

## Installation and Usage Instructions

Using NData in your project: To use NData as a library, add it as a dependency in your Rust project’s Cargo.toml. 

For example:

```
  [dependencies]
  ndata = "0.3" # Check crates.io for the latest version
```

To enable optional Serde support:

```
  [dependencies]
  ndata = { version = "0.3", features = ["serde_support"] }
```

After adding, run cargo build to fetch and compile the crate.

Once included, initialize the NData system exactly once at startup of your program. Call the function ndata::init("data") (or ndata::init("data")) before creating or using any NData objects. This sets up the global storage heaps in memory. 

For example:

```
  fn main() {
      ndata::init("data");  // Initialize global NData storage (call only once)
      // ... your code ...
  }
```

**Note: If you forget to call init(), the first attempt to use NData’s global storage will panic or fail, because the static heaps are not set.**

After initialization, you can create and manipulate NData structures. The API revolves around a few core types:

* Data: an enum that can hold any value type (object, array, int, float, etc.).

* DataObject: a map/dictionary type (keys are String, values are Data).

* DataArray: a dynamic array/list of Data values.

* DataBytes: a byte buffer type.

For example, to create an object you can do let obj = DataObject::new();. You can then insert properties into it. A property is set by providing a Data value. 

For instance:

```
  obj.set_property("name", Data::DString("Alice".into()));
  obj.set_property("age", Data::DInt(30));
```

Here, "name" and "age" become keys in the object, with values of type string and integer respectively. You can retrieve properties similarly (e.g., obj.get_string("name") or obj.get_int("age")). Internally, these calls lock the global heap, insert or fetch the value, and manage reference counts automatically.

## Memory cleanup:
NData does not free memory automatically on drop the way Rust’s normal types do. Instead, you must periodically or at program end call the garbage collector. Use ndata::gc() to perform garbage collection – this will free any values that are no longer referenced anywhere. For example, after you are done with a large data structure (or on a timer in a long-running service), call ndata::gc(). Until you call this, any released NData values remain in the internal heap (they are just marked for deletion). In other words, dropping a DataObject or DataArray will only mark it for GC; you still need to invoke gc() to reclaim the memory. This design gives you control over when the potentially expensive sweep runs.

If your application uses the hot-reload feature (swapping dynamic libraries at runtime within the same process), you will use two extra calls:


* `let config = ndata::init("data");` in the original library/code to initialize and also obtain an NDataConfig (which contains pointers/identifiers for the shared memory).

* In the newly loaded library/code, call `ndata::mirror(config);` to attach to the existing data heap that the original library/code created. This makes all the existing DataObjects/DataArrays available in the new library/code. Only do this once, at startup of the new code. After mirroring, you use the data normally. (Under the hood, mirror connects the static heaps in the new library/code to the memory from the old library/code.)

## Running the examples:
The repository includes a number of example programs (under the examples/ folder) illustrating various uses of NData (doubly linked list, multithreading, global variables, panic handling, hot-reload, etc.). If you have cloned the GitHub repo, you can run these examples with Cargo. For instance, to run the Doubly Linked List example:

```
  cargo run --example doublylinkedlist
```

This will compile and execute examples/doublylinkedlist/src/main.rs, which demonstrates using a DataObject to implement a linked list (storing pointers to “next” and “prev” as DataObjects). Similarly, cargo run --example multithreaded will show usage of NData across threads, globals shows using NData as a global state store, and so on. These examples are a great way to see how the API is used in practice.

Code Structure and Architecture

The NData codebase is organized into several modules, each handling a different aspect of the data structure implementation. The main components of the code and their roles are:

* src/ndata.rs: 
  
  This is the library’s root module (lib). It defines the high-level interface (the NData struct or namespace with functions). In particular, it provides the ndata::init, ndata::gc, ndata::mirror, and ndata::print_heap functions that coordinate the global behavior. It also defines the NDataConfig struct used when mirroring data between dynamic libraries within the same process. Essentially, this module ties together the others – when you call ndata::init("data") it internally calls the init functions of the DataObject, DataArray, and DataBytes modules to set up all their heaps.
  
* src/data.rs: 
  
  This module defines the core Data enum type, which represents any value in the NData system. It has variants for each supported type: DObject(usize), DArray(usize), DBytes(usize), DString(String), DFloat(f64), DInt(i64), DBoolean(bool), and DNull. The three variants with usize payload (DObject, DArray, DBytes) are essentially handles or references into the internal heaps – the usize is an index or pointer to the actual object/array/bytes stored in NData’s heap. For example, Data::DObject(n) means “an object located at index n in the object heap”. The other variants (string, float, int, bool, null) carry the value directly. The data.rs module also implements numerous utility methods on Data (type-checking methods like is_object, accessors like as_string or object() which downcast to DataObject, etc.). This allows easy conversion between the enum and the structured types.

* src/dataobject.rs: 

  This module implements the DataObject type, NData’s dynamic object (key-value map). Internally, a DataObject is defined as a struct holding a data_ref: usize field. This data_ref is the index of this object’s storage in the global object heap. The actual data (the map of key to value) lives in a global static heap, and data_ref is like a pointer to it. The DataObject module declares a global static OH (object heap) of type Heap<HashMap<String, Data>> and a static OD (object drop list) for pending deletions. It provides functions to manipulate objects: e.g., DataObject::new() allocates a new empty HashMap in the heap and returns a DataObject pointing to it, set_property(&mut self, key, Data) inserts a value into the map, get_property(&self, key) retrieves a value, and convenience getters like get_int, get_string convert the result to a Rust primitive. DataObject’s Drop implementation is overridden so that when a DataObject goes out of scope, it does not immediately free the map; instead it pushes its data_ref onto the OD drop list (to be collected later). There’s also a Clone impl: cloning a DataObject simply creates a new handle to the same data_ref and increments the reference count for that object in the heap. (Thus, multiple DataObject instances can point to the same underlying map; the heap’s ref count ensures it stays alive until all are dropped.) The module also includes an init() function to initialize the static OH heap (by calling Heap::new()) and a gc() function to actually free any objects whose ref count dropped to zero (processing the OD list). Similar structure and logic apply to DataArray and DataBytes below.

* src/dataarray.rs: 

  This defines DataArray, the dynamic array type. It parallels DataObject in structure. Internally it has pub data_ref: usize pointing into a global array heap AH (of type Heap<Vec<Data>>) and a drop list AD. A new DataArray will allocate a fresh Vec<Data> in the heap. Methods like push(&mut self, Data) or index access are provided to manipulate the array. Dropping or cloning a DataArray works the same way (ref counts managed in the heap, indices pushed to AD on drop). DataArray also has its own init() and gc() functions that the top-level ndata::init/gc will call.

* src/databytes.rs: 

  This defines DataBytes, used for binary data (a buffer of u8). It again uses the pattern of a global heap BH: Heap<Vec<u8>> and drop list BD to store byte buffers. The DataBytes struct holds a data_ref: usize into that heap. One can create a DataBytes from a Rust byte slice, and it will copy the bytes into the global heap. Methods allow reading/writing the bytes. Cloning and drop similarly affect reference counts and use a deletion queue.

* src/heap.rs: 

  This module implements the generic Heap<T> container that underlies the object/array/bytes storage. Heap<T> is essentially a reference-counted object pool for values of type T. Internally, it contains a vector of T and parallel structures to track reference counts (for each index in use) and free slots. It provides methods: push(value: T) to allocate a new value in the heap (returns a usize handle), get(handle) to get a reference to a value by index, incr(handle) and decr(handle) to increment/decrement the ref count of an entry, and try_get or keys for iteration/debugging. The Heap is responsible for freeing the storage when ref count reaches zero if garbage collection is invoked. In other words, when you call ndata::gc(), it will call Heap.decr on each item slated for deletion and actually remove those with count zero. The Heap is a central piece enabling the manual GC: it bundles an array of objects with their ref counts.

* src/sharedmutex.rs: 

  This defines the SharedMutex<T> type used for thread synchronization. It wraps a data value of type T and provides locking (similar to Mutex<T>). The key difference is that SharedMutex is designed to be used as a global (static) and potentially across dynamic library boundaries (within the same process) when using the mirror feature. The implementation uses an UnsafeCell and an OS locking primitive internally. For example, OH, OD, AH, etc. are all SharedMutex statics. To access the data, code calls .lock() on the SharedMutex, which returns a guard that allows safe mutable access to the inner heap or vector. This custom mutex is what makes the entire NData system thread-safe (all operations on global data are serialized through these locks) and also allows the global nature (since a normal Mutex in Rust cannot be easily used as a static mut across threads without initialization concerns). The mirror support enables SharedMutex to transform its internal pointer when mirror() is called, pointing it at an existing memory location accessible by different dynamic libraries within the same process.

* src/usizemap.rs: 

  This is a small internal utility module, a map keyed by usize. It is used to map heap indices to references or for debugging, optimizing certain lookups, or ensuring unique keys.

* src/json_util.rs: 

  Contains fallback routines for JSON parsing/printing when Serde is not enabled (i.e., the "serde_support" feature is not active). It can convert NData structures to a JSON string or vice versa using basic logic.

## Architecture in operation: 
At runtime, NData essentially maintains three global heaps (for objects, arrays, and byte buffers). Each heap is protected by a SharedMutex (to ensure only one thread at a time can modify it) and accompanied by a global list of pending deletions. When you create a new DataObject, for example, the process is: lock the object heap, allocate a new HashMap in it, get back an index, and store that index in your DataObject handle. That handle (data_ref) is how all future operations find the actual data. If you clone the DataObject, the code will increment the reference count in the heap entry. If you drop a DataObject, it does not immediately free the HashMap; instead it locks the deletion list (OD) and pushes the index onto it. Nothing is freed yet – the memory is still in the heap, possibly still referenced by other Data handles.

When ndata::gc() is called, it will lock each heap and its drop list in turn and process them. For each index in the drop list, the reference count in the heap is decremented (Heap.decr). If an entry’s count drops to zero, that entry is removed from the heap (freed). This design is a form of manual reference-counting garbage collection: reference counts accumulate during normal operations, but actual cleanup is deferred until an explicit collection phase. The heaps also provide a print_heap (accessible via ndata::print_heap()) which will print out the contents of each heap for debugging, so you can see how many objects/arrays are allocated at a given time.

### Interactions and data flow: 
The Data enum ties everything together – for instance, a DataObject can contain nested Data values (including other objects or arrays). If you insert a Data::DObject(child_ref) into a parent object, effectively you are creating a graph of references in the heap. NData handles this by simply storing the Data (which carries the child’s index) inside the parent’s HashMap. The reference count of the child object’s heap entry will be incremented to account for this new reference (this happens in set_property: if you insert a Data that is an object/array/bytes, NData will call the appropriate incr on that inner data’s index). In this way, the reference graph is tracked at the heap level – every time an object is stored inside another or cloned, its count increases; every time it’s removed or goes out of scope, the count is scheduled to decrease. This ensures that when gc() runs, it knows which items are truly unused.

### Thread safety and concurrency: 
Because all operations on NData structures acquire a lock on the corresponding SharedMutex heap, the architecture is inherently safe for concurrent use. For example, if two threads try to modify the same DataObject or even different DataObjects, they will both invoke oheap().lock(), causing one to wait until the other is done. This coarse locking (one mutex for all objects, one for all arrays, etc.) means the design sacrifices some parallelism for simplicity – only one thread can manipulate any object in the heap at a time. However, it prevents data races entirely. From the user’s perspective, NData objects can be shared freely among threads (they are Send + Sync by virtue of the internal locking). The global nature also means you don’t pass ownership; any thread can access the same global data by just holding a handle to it. This design aligns with the project’s goal of making Rust behave a bit more like a GC language in a multi-threaded context (no ownership headaches). The custom SharedMutex is an enabler of this, and with the mirror feature, it even extends to multi-dynamic-library cases (within the same process).

### Memory sharing (mirror) implementation: 
When the "mirror" feature is used, NData’s init functions return raw pointers/identifiers for the heaps. These are used by a newly loaded dynamic library to attach to the existing memory within the same process. The Heap struct has support for turning its internal vector into a shared memory segment (and Heap::share() returns a tuple of identifiers needed to find that segment). Heap::mirror(ptr1, ptr2) then maps or points the new dynamic library’s heap struct to the existing data. Similarly, SharedMutex::mirror ensures the lock is pointing to the same underlying lock (or is disabled if not needed). The result is that different dynamic libraries within the same process can see the same data. This is a rather advanced feature, but it underpins the hot-reload example: the newly loaded dynamic library calls ndata::mirror(config_from_parent) to continue where the previous library left off. From an architecture viewpoint, this means NData’s heaps are not just simple static globals – they are capable of being shared between dynamic libraries within the same process, which influenced their implementation (using raw pointers under the hood).

### Design patterns and principles: 
The architecture of NData can be seen as an implementation of a singleton memory pool with reference counting GC. Each of the data types (object/array/bytes) acts like an object pool of its instances. The use of global static singletons (OH, AH, BH) is akin to the Singleton pattern for managing a resource (in this case, a heap for each type) that is accessible application-wide. The manual GC follows the Deferred Reference Counting pattern – similar to how some garbage collectors defer work to specific times, NData defers freeing until an explicit call, to avoid the overhead during normal operations. This is a deliberate design to improve performance when lots of temporary clones happen (they won’t constantly free memory immediately, avoiding thrashing; you can collect in batches).

### Rust ownership model: 
By moving all actual data to a static heap and referring to it by indices, NData effectively circumvents Rust’s borrow checker for those values. Everything is treated as owned by the global heap, and the borrow checker is satisfied because you always work with a DataObject handle (which is just a small struct with an index). The heavy lifting of ensuring safety is done at runtime (via locks and refcounts) rather than compile-time. This is an intentional relaxation of Rust’s strict rules to gain flexibility. This approach is meant for scenarios where the strict ownership model is a hindrance, and it's recommended to eventually “refactor them out” for pure Rust solutions once the prototyping phase is over.

In summary, NData’s architecture consists of global locked heaps that store all data, and a thin layer of types (DataObject, DataArray, etc.) that act as handles into those heaps. It uses manual reference counting to manage lifetimes, with an explicit garbage collection call to reclaim memory. The structure is modular (separating the logic of different types and the generic heap and mutex functionality), and is designed with extension in mind (features for no_std and dynamic library mirroring within the same process). This design achieves the project’s goals: it allows the developer to treat data in a high-level way (like a Python dictionary or JSON) while the library takes care of memory and thread safety behind the scenes, all without default third-party dependencies and with no_std compatibility for embedded use. The cost is that the developer must be mindful to call gc() and understand that this is not idiomatic Rust (it's a trade-off for convenience). Nevertheless, for the intended use cases – quick prototypes, global state management, multi-threaded scenarios, and embedded applications – NData provides a robust and cleverly engineered solution within the Rust language constraints.

# Examples

## Rapid Prototyping
Rust's memory management and type safety can add significant up-front development 
effort when compared to languages with built-in garbage collection and relaxed 
type safety. NData restores those benefits, making Rust the perfect language for 
rapid prototyping. Yes, this does re-introduce the performance penalties and 
potential bugs, but unlike those other languages you can easily refactor them out 
of your code once you've worked out the logic.

*Example: [Doubly Linked List](examples/doublylinkedlist/src/main.rs)*

## Multithreaded Environments
NData is thread-safe by design. Objects can easily be shared between threads 
without worrying about ownership or mutexes, etc. 

*Example: [Multi-threaded](examples/multithreaded/src/main.rs)*

## Global Variables
Global variables are discouraged, and with good reason. However, sometimes you 
need them anyway-- and you shouldn't have to dork around with unsafe code, 
pointers, boxes, cells, and whatnot just to throw some data on the heap.

*Example: [Globals](examples/globals/src/main.rs)*

## Panic Management
Self-owned structures are not lost when panics occur. They are a convenient way to 
recover key information when unwinding a panic.

*Example: [Panic](examples/panic/src/main.rs)*

## Garbage Collection
NData adds Toaster-Simple™ garbage collection to Rust.

*Example: [Garbage Collection](examples/garbage-collection/src/main.rs)*

## Hotswap Live Code
NData works well in conjunction with the 
[hot-lib-reloader crate](https://crates.io/crates/hot-lib-reloader),
allowing you to swap new code into your running app  as you write it while 
maintaining the state of your runtime variables. 

*Example: [Hot-Reload](examples/hot-reload/src/main.rs)*

# LLM Usage

The following is a pretty good system prompt for LLMs that need to use the ndata crate:
```
You are an AI assistant working with the ndata crate in Rust. Key points:

Core: ndata provides globally shared, thread-safe, JSON-like dynamic data structures:
Data: Enum (DObject(usize), DArray(usize), DBytes(usize), DString, DInt, DFloat, DBoolean, DNull).
DataObject: Handle to a heap-stored HashMap<String, Data>.
DataArray: Handle to a heap-stored Vec<Data>.
DataBytes: Handle to a heap-stored DataStream (containing Vec<u8>, read/write state, MIME type).
All are identified by a data_ref: usize (index into global heaps).
⚠️ Critical Rule: NO WRAPPING: DataObject, DataArray, DataBytes, and Data are already internally reference-counted and thread-safe (via SharedMutex). Do NOT wrap them in Arc, Rc, Mutex, RwLock, etc. This causes double-locking and bugs.
Initialization & GC:
The conventional data storage location is a folder named "data" in the working directory, specified by ndata::init
Call ndata::init("data") once at application startup.
Garbage collection is manual. Dropping a DataObject, DataArray, or DataBytes handle queues its data_ref for deletion. Actual memory reclamation only occurs when ndata::gc() is called.
Handles & Reference Counting:
Instances of DataObject, DataArray, DataBytes are lightweight handles.
clone() on these handles increments the reference count for their shared data. Drop (when handles go out of scope) correctly queues the data_ref for potential garbage collection (which will decrement the count).
Mutator methods (e.g., put_object, set_property, push_array) automatically manage reference counts of items being inserted or replaced.
Simplicity: If you primarily use the specific typed methods on DataObject, DataArray, and DataBytes (e.g., obj.put_string(), arr.get_array(), bytes.clone()) and avoid direct manipulation of the Data enum or generic get_property/set_property with complex types, you generally do not need to call incr() or decr() manually. The framework handles it.
To duplicate data into a new instance (new data_ref, deep content copy), use deep_copy(). shallow_copy() creates a new instance but shares references to nested ndata types (incrementing their counts).
Common Operations & Distinct Patterns:
Creation:
DataObject::new(), DataArray::new(), DataBytes::new() create empty structures, returning a handle with a reference count of 1 for the new data.
DataBytes::from_bytes(&Vec<u8>) creates from byte data.
DataObject::get(ref), DataArray::get(ref), DataBytes::get(ref) retrieve existing handles by data_ref and increment their reference count.
Accessors:
DataObject: get_string("key"), get_object("key") -> DataObject, etc. Avoid get_property("key") -> Data if a typed getter exists.
DataArray: get_string(idx), get_array(idx) -> DataArray, etc. Avoid get_property(idx) -> Data if a typed getter exists.
DataBytes: get_data() -> Vec<u8>, current_len(), is_read_open(), to_hex_string() -> String.
Mutators (Setting/Pushing Values):
Typed helpers: obj.put_string("key", "val"), arr.push_int(123).
Nesting ndata Types (CRUCIAL PATTERN): When assigning an ndata type (e.g., DataObject, DataArray, DataBytes) as a property of another using its typed "put" or "push" methods:
Example: `parent_obj.put_object("child_key", child_obj);` or `parent_arr.push_array(child_arr);`
1. The `child_obj` (or `child_arr`, `child_bytes`) is **moved** into the method (e.g., `put_object`).
2. The method internally stores the `child_obj.data_ref` (e.g., as `Data::DObject(child_obj.data_ref)`) and **increments its reference count** in the heap. This is because the parent structure now holds a reference to this data.
3. Because the `child_obj` instance was moved into the method, it goes out of scope at the end of that method. Its **`Drop` implementation runs normally**.
4. The `Drop` implementation queues `child_obj.data_ref` for a **decrement** operation during the next garbage collection cycle.
5. This sequence (increment by the method, followed by a decrement from the drop of the moved-in handle) correctly balances the reference count. The net effect is that one reference to the data (originally held by the `child_obj` variable) is now held by the parent structure.
6. **Important**: Because `child_obj` was **moved** into the method, the original variable `child_obj` in the calling scope is **no longer valid and cannot be used further**, as per standard Rust ownership rules. If you need to continue using `child_obj` independently after putting it into a parent, you should `clone()` it first: `parent_obj.put_object("child_key", child_obj.clone());`. The clone creates a new handle and increments the ref count; this new handle is then moved and consumed by `put_object`, maintaining correct counts.
JSON Serialization/Deserialization (Default/json_util Fallback):
Used when serde_support feature is off, via DataObject::from_string() / ::to_string() and DataArray::from_string() / ::to_string().
Distinct DataBytes Handling: json_util serializes DataBytes to/from a hexadecimal string (e.g., "48 65 6C 6C 6F" for "Hello"). This is different from typical serde approaches (like Base64 or array of numbers).
Parsing is strict; json_util::{object,array}_from_string return Result<_, ParseError>.
Infer other method details based on these patterns. Focus on correct reference management (especially how moves and clones interact with reference counting when nesting types), the "no wrap" rule, and adhering to standard Rust ownership principles.
```
