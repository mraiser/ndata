//! A shareable reader-writer spinlock mutex implementation.
//! Original spinlock logic credit: Mikhail Panfilov
//! Reader-writer lock logic adapted for this structure.

use core::cell::UnsafeCell;
use core::hint::spin_loop;
use core::ops::{Deref, DerefMut};
use core::ptr;
use core::sync::atomic::{AtomicUsize, AtomicPtr, Ordering};

// Define constants for pointer types for clarity
type LockPtr = *const AtomicUsize;
type DataPtr<T> = *const UnsafeCell<T>;

// Constants for lock states
const UNLOCKED: usize = 0;
const WRITE_LOCKED: usize = usize::MAX; // Sentinel for write lock. Max readers = WRITE_LOCKED - 1

/// Represents the state of the SharedMutex: uninitialized, managing local data, or mirroring another mutex.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MutexState {
    /// The mutex has not been initialized with data or mirrored yet.
    Uninitialized,
    /// The mutex manages its own lock and data locally.
    Local,
    /// The mutex mirrors the state and data of another SharedMutex via raw pointers.
    Mirrored,
}

/// A reader-writer spinlock mutex that can potentially be shared across memory partitions.
///
/// This mutex uses atomic operations for locking and `UnsafeCell` for interior mutability.
/// It allows multiple concurrent readers OR one exclusive writer.
///
/// # Safety
/// This mutex relies on raw pointers for its "mirroring" functionality (`share` and `mirror` methods).
/// This is **inherently unsafe** and imposes strict requirements on the user:
///
/// 1.  **Non-Movement:** The `SharedMutex` instance that calls `set` (the "original" instance)
///     **must not be moved** in memory after `set` is called and while any mirrored instances exist.
///     Moving the original instance will invalidate the pointers shared via `share`, leading to
///     dangling pointers and **undefined behavior** in mirrored instances. Placing the original
///     `SharedMutex` in a `static` variable (often managed by `GlobalSharedMutex`) is the safest way
///     to ensure this non-movement requirement.
/// 2.  **Pointer Validity:** The pointers shared via `share` and used by `mirror` must remain valid
///     for the entire lifetime of the mirrored mutexes. The memory they point to (part of the
///     original `SharedMutex`) must not be deallocated or become invalid (e.g., by dropping
///     the original mutex prematurely).
/// 3.  **Memory Accessibility:** The caller is responsible for ensuring that the memory partitions
///     or contexts where mirrored mutexes operate can safely and correctly access the memory
///     locations specified by the shared pointers. This often depends on the system architecture
///     and memory model.
/// 4.  **Initialization Synchronization:** Calls to `set` or `mirror` on the *same* `SharedMutex`
///     instance must be properly synchronized if they can happen concurrently with other
///     operations (like `lock`, `read`, or `share`). This mutex does not provide internal synchronization
///     for its own initialization.
///
/// **Failure to meet these conditions will result in undefined behavior.** Use the `share`
/// and `mirror` features with extreme caution and only when the safety requirements can be
/// strictly guaranteed. For standard concurrent programming within a single process, prefer
/// `std::sync::RwLock` or other safer abstractions from the standard library.
#[derive(Debug)]
pub struct SharedMutex<T> {
    /// Pointer to the atomic lock state (`AtomicUsize`).
    lock_ptr: LockPtr,
    /// Pointer to the `UnsafeCell` containing the data `T`.
    data_ptr: DataPtr<T>,
    /// Tracks whether the mutex is local, mirrored, or uninitialized.
    state: MutexState,
    /// The storage for the lock state when the mutex is `Local`.
    local_lock_storage: AtomicUsize,
    /// The storage for the data (`T`) wrapped in `UnsafeCell` when the mutex is `Local`.
    local_data_storage: Option<UnsafeCell<T>>,
}

// Default implementation creates an uninitialized mutex.
impl<T> Default for SharedMutex<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> SharedMutex<T> {
    /// Creates a new, uninitialized `SharedMutex`.
    #[inline]
    pub const fn new() -> SharedMutex<T> {
        SharedMutex {
            lock_ptr: ptr::null(),
            data_ptr: ptr::null(),
            state: MutexState::Uninitialized,
            local_lock_storage: AtomicUsize::new(UNLOCKED),
            local_data_storage: None,
        }
    }

    /// Initializes the mutex with the given data `t`, making it a "local" mutex.
    pub fn set(&mut self, t: T) {
        if self.state != MutexState::Uninitialized {
            panic!("SharedMutex may only be initialized once (using set or mirror)");
        }
        self.local_data_storage = Some(UnsafeCell::new(t));
        self.local_lock_storage.store(UNLOCKED, Ordering::Relaxed);
        self.lock_ptr = &self.local_lock_storage as *const AtomicUsize;
        self.data_ptr = self.local_data_storage.as_ref().unwrap() as *const UnsafeCell<T>;
        self.state = MutexState::Local;
    }

    /// Returns the raw memory addresses of the lock state and the data cell.
    pub fn share(&self) -> (u64, u64) {
        if self.state != MutexState::Local {
            panic!("Only a locally set SharedMutex can be shared (must be initialized with `set`)");
        }
        debug_assert!(!self.lock_ptr.is_null(), "Internal error: null lock_ptr in Local state for share()");
        debug_assert!(!self.data_ptr.is_null(), "Internal error: null data_ptr in Local state for share()");
        (self.lock_ptr as u64, self.data_ptr as u64)
    }

    /// Initializes this mutex to mirror another `SharedMutex` using raw memory addresses.
    pub unsafe fn mirror(&mut self, lock_addr: u64, data_addr: u64) {
        if self.state != MutexState::Uninitialized {
            panic!("SharedMutex may only be initialized once (using set or mirror)");
        }
        if lock_addr == 0 || data_addr == 0 {
            panic!("Cannot mirror using null addresses (lock_addr={}, data_addr={})", lock_addr, data_addr);
        }
        self.lock_ptr = lock_addr as LockPtr;
        self.data_ptr = data_addr as DataPtr<T>;
        self.state = MutexState::Mirrored;
        self.local_data_storage = None;
    }

    /// Acquires an exclusive write lock, spinning until it becomes available.
    #[inline]
    pub fn lock(&self) -> SharedMutexGuard<'_, T> {
        if !self.is_initialized() {
            panic!("Cannot lock an uninitialized SharedMutex (call `set` or `mirror` first)");
        }
        debug_assert!(!self.lock_ptr.is_null(), "Internal error: null lock_ptr in lock()");
        debug_assert!(!self.data_ptr.is_null(), "Internal error: null data_ptr in lock()");
        loop {
            match unsafe { (*self.lock_ptr).compare_exchange_weak(
                UNLOCKED,
                WRITE_LOCKED,
                Ordering::Acquire,
                Ordering::Relaxed,
            )} {
                Ok(_) => return SharedMutexGuard { mutex: self },
                Err(_) => spin_loop(),
            }
        }
    }

    /// Acquires a shared read lock, spinning until it becomes available.
    #[inline]
    pub fn read(&self) -> SharedMutexReadGuard<'_, T> {
        if !self.is_initialized() {
            panic!("Cannot read-lock an uninitialized SharedMutex (call `set` or `mirror` first)");
        }
        debug_assert!(!self.lock_ptr.is_null(), "Internal error: null lock_ptr in read()");
        debug_assert!(!self.data_ptr.is_null(), "Internal error: null data_ptr in read()");
        loop {
            let current_state = unsafe { (*self.lock_ptr).load(Ordering::Relaxed) };
            if current_state == WRITE_LOCKED {
                spin_loop();
                continue;
            }
            if current_state == WRITE_LOCKED - 1 {
                // Max readers reached, extremely unlikely.
                spin_loop();
                continue;
            }
            match unsafe { (*self.lock_ptr).compare_exchange_weak(
                current_state,
                current_state + 1,
                Ordering::Acquire,
                Ordering::Relaxed,
            )} {
                Ok(_) => return SharedMutexReadGuard { mutex: self },
                Err(_) => spin_loop(),
            }
        }
    }

    /// Releases the exclusive write lock.
    #[inline]
    fn release_write_lock(&self) {
        debug_assert!(self.is_initialized(), "Attempted to release write lock on uninitialized mutex");
        debug_assert!(!self.lock_ptr.is_null(), "Attempted to release write lock with a null lock_ptr");
        unsafe { (*self.lock_ptr).store(UNLOCKED, Ordering::Release); }
    }

    /// Releases a shared read lock.
    #[inline]
    fn release_read_lock(&self) {
        debug_assert!(self.is_initialized(), "Attempted to release read lock on uninitialized mutex");
        debug_assert!(!self.lock_ptr.is_null(), "Attempted to release read lock with a null lock_ptr");
        unsafe { (*self.lock_ptr).fetch_sub(1, Ordering::Release); }
    }

    /// Checks if the mutex is currently locked.
    #[inline]
    pub fn is_locked(&self) -> bool {
        if !self.is_initialized() {
            panic!("Cannot check lock status of an uninitialized SharedMutex");
        }
        debug_assert!(!self.lock_ptr.is_null(), "Internal error: null lock_ptr in is_locked()");
        unsafe { (*self.lock_ptr).load(Ordering::Acquire) != UNLOCKED }
    }

    /// Checks if the mutex has been initialized.
    #[inline]
    pub fn is_initialized(&self) -> bool {
        self.state != MutexState::Uninitialized
    }
}

/// Guard for exclusive (write) access.
#[derive(Debug)]
#[must_use = "if unused the Mutex will immediately unlock"]
pub struct SharedMutexGuard<'a, T> {
    mutex: &'a SharedMutex<T>,
}

impl<T> Deref for SharedMutexGuard<'_, T> {
    type Target = T;
    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe {
            debug_assert!(self.mutex.is_initialized(), "WriteGuard exists for uninitialized mutex");
            debug_assert!(!self.mutex.data_ptr.is_null(), "WriteGuard exists with null data_ptr");
            &*(*self.mutex.data_ptr).get()
        }
    }
}

impl<T> DerefMut for SharedMutexGuard<'_, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe {
            debug_assert!(self.mutex.is_initialized(), "WriteGuard exists for uninitialized mutex");
            debug_assert!(!self.mutex.data_ptr.is_null(), "WriteGuard exists with null data_ptr");
            &mut *(*self.mutex.data_ptr).get()
        }
    }
}

impl<T> Drop for SharedMutexGuard<'_, T> {
    #[inline]
    fn drop(&mut self) {
        if self.mutex.is_initialized() {
            self.mutex.release_write_lock();
        }
    }
}

/// Guard for shared (read) access.
#[derive(Debug)]
#[must_use = "if unused the Mutex will immediately unlock"]
pub struct SharedMutexReadGuard<'a, T> {
    mutex: &'a SharedMutex<T>,
}

impl<T> Deref for SharedMutexReadGuard<'_, T> {
    type Target = T;
    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe {
            debug_assert!(self.mutex.is_initialized(), "ReadGuard exists for uninitialized mutex");
            debug_assert!(!self.mutex.data_ptr.is_null(), "ReadGuard exists with null data_ptr");
            &*(*self.mutex.data_ptr).get()
        }
    }
}

impl<T> Drop for SharedMutexReadGuard<'_, T> {
    #[inline]
    fn drop(&mut self) {
        if self.mutex.is_initialized() {
            self.mutex.release_read_lock();
        }
    }
}

// SAFETY: See previous detailed comments. The reasoning for Send/Sync remains the same.
unsafe impl<T: Send> Send for SharedMutex<T> {}
unsafe impl<T: Send> Sync for SharedMutex<T> {}
unsafe impl<'a, T: Send> Send for SharedMutexGuard<'a, T> {}
unsafe impl<'a, T: Send + Sync> Sync for SharedMutexGuard<'a, T> {}
unsafe impl<'a, T: Send> Send for SharedMutexReadGuard<'a, T> {}
unsafe impl<'a, T: Send + Sync> Sync for SharedMutexReadGuard<'a, T> {}

// =============================================================================
// GlobalSharedMutex Implementation (No OnceCell)
// =============================================================================

// Initialization states for GlobalSharedMutex
const GLOBAL_UNINITIALIZED: usize = 0;
const GLOBAL_INITIALIZING: usize = 1;
const GLOBAL_INITIALIZED: usize = 2;

/// A wrapper around `SharedMutex` for convenient global static initialization and access,
/// implemented without external dependencies like `once_cell`.
///
/// This uses `AtomicUsize` for state tracking and `AtomicPtr` to hold the `SharedMutex`.
/// The `SharedMutex` is heap-allocated via `Box` and its pointer is stored.
/// For `static` instances, the memory for the `SharedMutex` is intentionally leaked,
/// which is a common pattern for `static`s requiring heap allocation without `Drop`
/// being called (as `static`s don't drop by default).
///
/// # Example
/// ```
/// # use std::thread;
/// # // Assuming TestData is defined elsewhere or in scope for the example
/// # #[derive(Debug, Default, Clone, PartialEq)] pub struct TestData { value: i32, text: String }
/// # // Use the actual crate name if this were in a library, e.g., `my_mutex_crate::GlobalSharedMutex`
/// # use self::shared_mutex_with_global::{GlobalSharedMutex, SharedMutexGuard, SharedMutexReadGuard};
///
/// static MY_GLOBAL_DATA: GlobalSharedMutex<TestData> = GlobalSharedMutex::new();
///
/// fn main() {
///     MY_GLOBAL_DATA.init(TestData { value: 10, text: "hello".to_string() });
///
///     thread::spawn(|| {
///         let mut guard = MY_GLOBAL_DATA.lock();
///         guard.value += 1;
///         guard.text.push_str(" world");
///     }).join().unwrap();
///
///     let guard = MY_GLOBAL_DATA.read();
///     assert_eq!(guard.value, 11);
///     assert_eq!(guard.text, "hello world");
/// }
/// ```
#[derive(Debug)]
pub struct GlobalSharedMutex<T> {
    state: AtomicUsize,
    ptr: AtomicPtr<SharedMutex<T>>,
}

impl<T> GlobalSharedMutex<T> {
    /// Creates a new, uninitialized `GlobalSharedMutex`.
    /// This function is `const`, suitable for `static` variable initialization.
    pub const fn new() -> Self {
        Self {
            state: AtomicUsize::new(GLOBAL_UNINITIALIZED),
            ptr: AtomicPtr::new(ptr::null_mut()),
        }
    }

    /// Initializes the global mutex with the given data.
    /// This method ensures the `SharedMutex` is initialized exactly once.
    ///
    /// # Panics
    /// Panics if `init` is called more than once on the same `GlobalSharedMutex` instance.
    pub fn init(&self, data: T) {
        // Attempt to transition from UNINITIALIZED to INITIALIZING
        match self.state.compare_exchange(
            GLOBAL_UNINITIALIZED,
            GLOBAL_INITIALIZING,
            Ordering::Acquire, // Acquire to synchronize with other potential initializers
            Ordering::Relaxed, // Relaxed on failure, we'll check the actual state
        ) {
            Ok(_) => { // Successfully transitioned to INITIALIZING, this thread does the work
                // 1. Create the SharedMutex on the heap first.
                //    SharedMutex::new() initializes local_data_storage to None, and pointers to null.
                let mut boxed_sm = Box::new(SharedMutex::<T>::new());

                // 2. Call `set` on the heap-allocated SharedMutex.
                //    `set` will correctly initialize `local_data_storage` within the Box,
                //    and `lock_ptr`/`data_ptr` will point to locations *within the Box on the heap*.
                boxed_sm.set(data); // `data` is moved into the Boxed SharedMutex

                // 3. Store the raw pointer. Box::into_raw leaks the Box.
                self.ptr.store(Box::into_raw(boxed_sm), Ordering::Release);

                // Mark as INITIALIZED
                self.state.store(GLOBAL_INITIALIZED, Ordering::Release); // Release to publish the ptr and state
            }
            Err(current_state) => {
                if current_state == GLOBAL_INITIALIZING {
                    while self.state.load(Ordering::Acquire) == GLOBAL_INITIALIZING {
                        core::hint::spin_loop();
                    }
                    if self.state.load(Ordering::Relaxed) != GLOBAL_INITIALIZED {
                        panic!("GlobalSharedMutex failed to initialize correctly after spinning.");
                    }
                } else if current_state == GLOBAL_INITIALIZED {
                    panic!("GlobalSharedMutex::init called more than once or on an already initialized mutex.");
                } else {
                    panic!("GlobalSharedMutex in unexpected state during init: {}", current_state);
                }
            }
        }
    }

    /// Gets a reference to the underlying `SharedMutex`.
    /// Spins if initialization is in progress.
    /// # Panics
    /// Panics if the `GlobalSharedMutex` has not been initialized.
    #[inline]
    fn get_mutex(&self) -> &SharedMutex<T> {
        loop {
            match self.state.load(Ordering::Acquire) {
                GLOBAL_INITIALIZED => {
                    let ptr = self.ptr.load(Ordering::Acquire);
                    // SAFETY:
                    // 1. ptr is non-null if state is INITIALIZED because init() stores it.
                    // 2. ptr was obtained from Box::into_raw and points to a valid SharedMutex<T>.
                    // 3. The SharedMutex<T> lives as long as the GlobalSharedMutex (leaked for statics).
                    // 4. Access is read-only (&SharedMutex<T>), and SharedMutex itself handles internal sync.
                    // 5. Acquire ordering ensures we see the initialized ptr.
                    debug_assert!(!ptr.is_null(), "GlobalSharedMutex ptr is null despite being initialized");
                    return unsafe { &*ptr };
                }
                GLOBAL_INITIALIZING => {
                    spin_loop(); // Wait for initialization to complete
                }
                GLOBAL_UNINITIALIZED => {
                    panic!("GlobalSharedMutex has not been initialized. Call init() first.");
                }
                _ => unreachable!("GlobalSharedMutex in invalid state"),
            }
        }
    }

    /// Acquires an exclusive write lock. See `SharedMutex::lock()`.
    /// # Panics
    /// Panics if `init()` has not been called.
    pub fn lock(&self) -> SharedMutexGuard<'_, T> {
        self.get_mutex().lock()
    }

    /// Acquires a shared read lock. See `SharedMutex::read()`.
    /// # Panics
    /// Panics if `init()` has not been called.
    pub fn read(&self) -> SharedMutexReadGuard<'_, T> {
        self.get_mutex().read()
    }

    /// Returns raw memory addresses for mirroring. See `SharedMutex::share()`.
    /// # Panics
    /// Panics if `init()` has not been called.
    pub fn share(&self) -> (u64, u64) {
        self.get_mutex().share()
    }

    /// Checks if the underlying mutex is locked. See `SharedMutex::is_locked()`.
    /// # Panics
    /// Panics if `init()` has not been called.
    pub fn is_locked(&self) -> bool {
        self.get_mutex().is_locked()
    }
}

// SAFETY for GlobalSharedMutex<T>:
// `GlobalSharedMutex<T>` uses `AtomicUsize` and `AtomicPtr`. These are Send/Sync.
// The `SharedMutex<T>` pointed to is `Send + Sync` if `T: Send`.
// The `init` method uses atomic operations to ensure safe one-time initialization and publication
// of the `SharedMutex<T>` pointer.
// The `get_mutex` method uses atomic loads with Acquire ordering to ensure visibility.
// The raw pointer is obtained from `Box::into_raw`, and for static `GlobalSharedMutex` instances,
// this memory is leaked, ensuring the pointer remains valid for the program's lifetime.
// Therefore, `GlobalSharedMutex<T>` is `Send` and `Sync` if `T` is `Send`.
unsafe impl<T: Send> Send for GlobalSharedMutex<T> {}
unsafe impl<T: Send> Sync for GlobalSharedMutex<T> {}

// Note: If GlobalSharedMutex instances were not 'static and could be dropped,
// a Drop impl would be needed to call Box::from_raw to free the SharedMutex.
// For 'static usage, leaking is the standard approach without external crates.

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    #[derive(Debug, Default, Clone, PartialEq)]
    pub struct TestData {
        pub value: i32,
        pub text: String,
    }

    // ... (Original SharedMutex tests remain unchanged) ...
    #[test]
    fn basic_write_lock_unlock() {
        let mut mutex = SharedMutex::new();
        mutex.set(TestData { value: 10, text: "hello".to_string() });

        {
            let mut guard = mutex.lock(); // Write lock
            assert_eq!(guard.value, 10);
            guard.value = 20;
            guard.text = "world".to_string();
        } // Write lock released

        {
            let guard = mutex.lock(); // Re-acquire write lock
            assert_eq!(guard.value, 20);
            assert_eq!(guard.text, "world");
        }
    }

    #[test]
    fn basic_read_lock_unlock() {
        let mut mutex = SharedMutex::new();
        mutex.set(TestData { value: 30, text: "read test".to_string() });

        {
            let guard = mutex.read(); // Read lock
            assert_eq!(guard.value, 30);
            assert_eq!(guard.text, "read test");
        } // Read lock released

        // Multiple readers
        let r1 = mutex.read();
        let r2 = mutex.read();
        assert_eq!(r1.value, 30);
        assert_eq!(r2.value, 30);
        drop(r1);
        drop(r2);
    }

    #[test]
    fn write_blocks_read() {
        let mut m = SharedMutex::new();
        m.set(TestData::default());
        let mutex = Arc::new(m);

        let writer_mutex_ref = Arc::clone(&mutex);
        let _write_guard = writer_mutex_ref.lock();

        let reader_mutex_ref = Arc::clone(&mutex);
        let reader_thread = thread::spawn(move || {
            let start_time = std::time::Instant::now();
            let _read_guard = reader_mutex_ref.read();
            assert!(start_time.elapsed() > Duration::from_millis(50), "Reader did not block for writer");
        });

        thread::sleep(Duration::from_millis(100));
        drop(_write_guard);

        reader_thread.join().unwrap();
    }

    #[test]
    fn read_blocks_write() {
        let mut m = SharedMutex::new();
        m.set(TestData::default());
        let mutex = Arc::new(m);

        let reader_mutex_ref = Arc::clone(&mutex);
        let _read_guard = reader_mutex_ref.read();

        let writer_mutex_ref = Arc::clone(&mutex);
        let writer_thread = thread::spawn(move || {
            let start_time = std::time::Instant::now();
            let mut _write_guard = writer_mutex_ref.lock();
            _write_guard.value = 100;
            assert!(start_time.elapsed() > Duration::from_millis(50), "Writer did not block for reader");
        });

        thread::sleep(Duration::from_millis(100));
        drop(_read_guard);

        writer_thread.join().unwrap();

        let final_read = mutex.read();
        assert_eq!(final_read.value, 100);
    }

    #[test]
    fn multiple_readers_concurrently() {
        let mut m = SharedMutex::new();
        m.set(TestData { value: 123, text: "concurrent".to_string() });
        let mutex = Arc::new(m);
        let barrier = Arc::new(std::sync::Barrier::new(5));
        let mut handles = vec![];

        for _i in 0..5 {
            let reader_mutex_ref = Arc::clone(&mutex);
            let barrier_clone = Arc::clone(&barrier);
            let handle = thread::spawn(move || {
                barrier_clone.wait();
                let guard = reader_mutex_ref.read();
                assert_eq!(guard.value, 123);
                assert_eq!(guard.text, "concurrent");
                thread::sleep(Duration::from_millis(50));
                drop(guard);
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn is_locked_behavior() {
        let mut mutex = SharedMutex::new();
        mutex.set(TestData::default());

        assert!(!mutex.is_locked(), "Should not be locked initially after set");

        let r_guard = mutex.read();
        assert!(mutex.is_locked(), "Should be locked after acquiring read lock");
        drop(r_guard);
        assert!(!mutex.is_locked(), "Should not be locked after read lock released");

        let w_guard = mutex.lock();
        assert!(mutex.is_locked(), "Should be locked after acquiring write lock");
        drop(w_guard);
        assert!(!mutex.is_locked(), "Should not be locked after write lock released");
    }

    #[test]
    fn shared_mutex_can_be_static_like() {
        let mut local_static_sim_owner = Box::new(SharedMutex::<i32>::new());
        local_static_sim_owner.set(100);

        let local_static_sim: &SharedMutex<i32> = &*local_static_sim_owner;

        let _r = local_static_sim.read();
        assert_eq!(*_r, 100);
        drop(_r);

        let mut _w = local_static_sim_owner.lock();
        *_w = 200;
        drop(_w);

        let _r2 = local_static_sim.read();
        assert_eq!(*_r2, 200);
    }

    #[test]
    fn mirror_test() {
        let mut original_mutex_owner = Box::new(SharedMutex::<TestData>::new());
        original_mutex_owner.set(TestData { value: 1000, text: "original".to_string() });

        let (lock_addr, data_addr) = original_mutex_owner.share();

        let mut mirrored_mutex = SharedMutex::<TestData>::new();
        unsafe {
            mirrored_mutex.mirror(lock_addr, data_addr);
        }

        {
            let guard = mirrored_mutex.read();
            assert_eq!(guard.value, 1000);
            assert_eq!(guard.text, "original");
        }
        {
            let mut guard = original_mutex_owner.lock();
            guard.value = 2000;
            guard.text = "modified by original".to_string();
        }
        {
            let guard = mirrored_mutex.read();
            assert_eq!(guard.value, 2000);
            assert_eq!(guard.text, "modified by original");
        }
        {
            let mut guard = mirrored_mutex.lock();
            guard.value = 3000;
            guard.text = "modified by mirror".to_string();
        }
        {
            let guard = original_mutex_owner.read();
            assert_eq!(guard.value, 3000);
            assert_eq!(guard.text, "modified by mirror");
        }
    }

    #[test]
    #[should_panic(expected = "SharedMutex may only be initialized once")]
    fn set_twice_panics() {
        let mut m = SharedMutex::<i32>::new();
        m.set(10);
        m.set(20);
    }

    #[test]
    #[should_panic(expected = "SharedMutex may only be initialized once")]
    unsafe fn mirror_after_set_panics() {
        let mut m1 = SharedMutex::<i32>::new();
        m1.set(10);
        let (l,d) = m1.share();

        let mut m2 = SharedMutex::<i32>::new();
        m2.set(20);
        m2.mirror(l,d);
    }

    #[test]
    #[should_panic(expected = "Cannot lock an uninitialized SharedMutex")]
    fn lock_uninitialized_panics() {
        let m = SharedMutex::<i32>::new();
        let _g = m.lock();
    }

    #[test]
    #[should_panic(expected = "Cannot read-lock an uninitialized SharedMutex")]
    fn read_uninitialized_panics() {
        let m = SharedMutex::<i32>::new();
        let _g = m.read();
    }

    #[test]
    #[should_panic(expected = "Only a locally set SharedMutex can be shared")]
    fn share_uninitialized_panics() {
        let m = SharedMutex::<i32>::new();
        m.share();
    }

    #[test]
    #[should_panic(expected = "Only a locally set SharedMutex can be shared")]
    unsafe fn share_mirrored_panics() {
        let mut original = Box::new(SharedMutex::<i32>::new());
        original.set(1);
        let (l,d) = original.share();

        let mut mirrored = SharedMutex::<i32>::new();
        mirrored.mirror(l,d);
        mirrored.share();
    }
}

#[cfg(test)]
mod global_tests {
    // Bring items from the parent module (which includes GlobalSharedMutex, TestData via super::tests::*)
    use super::*;
    use std::sync::Arc;
    use std::thread;
    // Duration is already in scope via super::* from std::time::Duration in tests module.

    // This static is specific to this test.
    // If tests run in parallel, each test needing a unique static should define its own.
    static GLOBAL_INT_MUTEX_FOR_BASIC_TEST: GlobalSharedMutex<i32> = GlobalSharedMutex::new();

    #[test]
    fn g_basic_init_lock_read() {
        // For test isolation, it's often better to create a new GlobalSharedMutex instance
        // rather than relying on a single static that might be mutated by other tests
        // if tests were to run in parallel and share statics without care.
        // However, this test demonstrates usage with a declared static.
        // The `init` method itself is designed to be called once.
        // If this test is run multiple times in the same process without restarting,
        // the second run would panic at `init` if it's the same static instance.
        // Cargo runs tests in a way that this is usually fine for separate test functions.
        let test_static_mutex: GlobalSharedMutex<i32> = GlobalSharedMutex::new();
        test_static_mutex.init(100); // Initialize this specific instance

        {
            let mut guard = test_static_mutex.lock();
            assert_eq!(*guard, 100);
            *guard = 200;
        }
        {
            let guard = test_static_mutex.read();
            assert_eq!(*guard, 200);
        }
    }

    #[test]
    #[should_panic(expected = "GlobalSharedMutex::init called more than once")]
    fn g_double_init_panics() {
        // This test uses the globally defined static.
        // It's important that this test runs in an environment where it can attempt the first init.
        // If another test already initialized GLOBAL_INT_MUTEX_FOR_BASIC_TEST, this test's behavior might change.
        // To make it robust, we use a local instance for this specific panic test.
        let temp_global: GlobalSharedMutex<i32> = GlobalSharedMutex::new();
        temp_global.init(1);
        temp_global.init(2); // Should panic
    }

    #[test]
    #[should_panic(expected = "GlobalSharedMutex has not been initialized")]
    fn g_lock_before_init_panics() {
        let temp_global: GlobalSharedMutex<i32> = GlobalSharedMutex::new();
        let _guard = temp_global.lock(); // Should panic
    }

    #[test]
    #[should_panic(expected = "GlobalSharedMutex has not been initialized")]
    fn g_read_before_init_panics() {
        let temp_global: GlobalSharedMutex<i32> = GlobalSharedMutex::new();
        let _guard = temp_global.read(); // Should panic
    }

    #[test]
    fn g_multithreaded_access() {
        let local_global_mutex: Arc<GlobalSharedMutex<i32>> = Arc::new(GlobalSharedMutex::new());
        local_global_mutex.init(0);

        let mut handles = vec![];

        for i in 0..10 {
            let mutex_clone = Arc::clone(&local_global_mutex);
            let handle = thread::spawn(move || {
                for _ in 0..100 {
                    let mut guard = mutex_clone.lock();
                    *guard += 1;
                    if i == 0 && *guard % 10 == 0 {
                        drop(guard);
                        let r_guard = mutex_clone.read();
                        assert!(*r_guard > 0);
                    }
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let final_guard = local_global_mutex.lock();
        assert_eq!(*final_guard, 10 * 100);
    }

    #[test]
    fn g_share_and_mirror_works() {
        let local_global_owner: GlobalSharedMutex<TestData> = GlobalSharedMutex::new();
        local_global_owner.init(TestData { value: 42, text: "global_shared".to_string() });

        let (lock_addr, data_addr) = local_global_owner.share();
        assert_ne!(lock_addr, 0);
        assert_ne!(data_addr, 0);

        let mut mirrored_mutex = SharedMutex::<TestData>::new();
        unsafe {
             mirrored_mutex.mirror(lock_addr, data_addr);
        }

        {
            let guard = mirrored_mutex.read();
            assert_eq!(guard.value, 42);
            assert_eq!(guard.text, "global_shared");
        }
        {
            let mut guard = local_global_owner.lock();
            guard.value = 123;
            guard.text = "modified_via_global".to_string();
        }
        {
            let guard = mirrored_mutex.read();
            assert_eq!(guard.value, 123);
            assert_eq!(guard.text, "modified_via_global");
        }
         {
            let mut guard = mirrored_mutex.lock();
            guard.value = 456;
            guard.text = "modified_via_mirror".to_string();
        }
        {
            let guard = local_global_owner.read();
            assert_eq!(guard.value, 456);
            assert_eq!(guard.text, "modified_via_mirror");
        }
    }

    #[test]
    fn g_is_locked_behavior() {
        let m: GlobalSharedMutex<i32> = GlobalSharedMutex::new();
        m.init(10);

        assert!(!m.is_locked());
        let r_guard = m.read();
        assert!(m.is_locked());
        drop(r_guard);
        assert!(!m.is_locked());

        let w_guard = m.lock();
        assert!(m.is_locked());
        drop(w_guard);
        assert!(!m.is_locked());
    }

    // Test to ensure that if one thread starts initializing, other threads wait.
    #[test]
    fn g_init_concurrent_access_waits() {
        let mutex: Arc<GlobalSharedMutex<i32>> = Arc::new(GlobalSharedMutex::new());
        let barrier = Arc::new(std::sync::Barrier::new(2));

        let mutex_clone1 = Arc::clone(&mutex);
        let barrier_clone1 = Arc::clone(&barrier);
        let thread1 = thread::spawn(move || {
            barrier_clone1.wait();
            mutex_clone1.init(123); // First thread initializes
            assert_eq!(*mutex_clone1.read(), 123);
        });

        let mutex_clone2 = Arc::clone(&mutex);
        let barrier_clone2 = Arc::clone(&barrier);
        let thread2 = thread::spawn(move || {
            barrier_clone2.wait();
            // This thread should wait if init is in progress, then successfully get the value
            // or panic if it tries to init again (which it shouldn't with this logic).
            // The get_mutex() will spin if state is INITIALIZING.
            let val = *mutex_clone2.read();
            assert_eq!(val, 123); // Should see the value initialized by thread1
        });

        thread1.join().unwrap();
        thread2.join().unwrap();
    }
}

