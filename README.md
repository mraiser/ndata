# NData
NData provides self-owned data structures supporting objects, arrays, strings, 
integers, floats, booleans, byte buffers, and null. DataObject, DataArray, and 
DataBytes instances maintain reference counts. Garbage collection is performed 
manually by calling the NData::gc() function.

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
