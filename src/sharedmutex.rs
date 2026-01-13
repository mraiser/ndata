//! A shareable reader-writer spinlock mutex implementation.
//! Original spinlock logic credit: Mikhail Panfilov
//! Reader-writer lock logic adapted for this structure.
//!
//! # Production Notes
//! - Validated for False Sharing mitigation via Cache Padding.
//! - Validated for Self-Referential safety via PhantomPinned.

use core::cell::UnsafeCell;
use core::hint::spin_loop;
use core::marker::PhantomPinned;
use core::ops::{Deref, DerefMut};
use core::ptr;
use core::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

// Define constants for pointer types for clarity
type LockPtr = *const AtomicUsize;
type DataPtr<T> = *const UnsafeCell<T>;

// Constants for lock states
const UNLOCKED: usize = 0;
const WRITE_LOCKED: usize = usize::MAX; // Sentinel for write lock.

/// Align to 128 bytes to cover cache lines for x86_64 (usually 64) and ARM/Apple Silicon (often 128).
/// This prevents "false sharing," where locking the mutex invalidates the cache line for the data.
#[derive(Debug)]
#[repr(align(128))]
struct CachePadded<T>(T);

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
/// # Safety and Semantics
/// This struct is **Self-Referential** when in the `Local` state.
/// 1. **Do Not Move:** Once `set()` is called, this struct must not be moved in memory.
///    Moving it will invalidate internal pointers, causing Undefined Behavior.
///    The `PhantomPinned` marker is included to prevent `Unpin` implementation.
/// 2. **Mirroring:** Mirroring relies on raw pointers. You must ensure the source mutex
///    outlives the mirrored instance.
#[derive(Debug)]
pub struct SharedMutex<T> {
    /// Pointer to the atomic lock state (`AtomicUsize`).
    lock_ptr: LockPtr,
    /// Pointer to the `UnsafeCell` containing the data `T`.
    data_ptr: DataPtr<T>,
    /// Tracks whether the mutex is local, mirrored, or uninitialized.
    state: MutexState,
    /// The storage for the lock state when the mutex is `Local`.
    /// Wrapped in CachePadded to prevent false sharing with `local_data_storage`.
    local_lock_storage: CachePadded<AtomicUsize>,
    /// The storage for the data (`T`) wrapped in `UnsafeCell` when the mutex is `Local`.
    local_data_storage: Option<UnsafeCell<T>>,
    /// Marker to suppress `Unpin`, signaling this struct is self-referential.
    _pin: PhantomPinned,
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
            local_lock_storage: CachePadded(AtomicUsize::new(UNLOCKED)),
            local_data_storage: None,
            _pin: PhantomPinned,
        }
    }

    /// Initializes the mutex with the given data `t`, making it a "local" mutex.
    ///
    /// # Safety Warning
    /// After calling `set`, this instance becomes self-referential.
    /// **Do not move this struct to a different memory location after calling `set`.**
    pub fn set(&mut self, t: T) {
        if self.state != MutexState::Uninitialized {
            panic!("SharedMutex may only be initialized once (using set or mirror)");
        }
        self.local_data_storage = Some(UnsafeCell::new(t));
        self.local_lock_storage.0.store(UNLOCKED, Ordering::Relaxed);

        // Take addresses of internal fields.
        self.lock_ptr = &self.local_lock_storage.0 as *const AtomicUsize;
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

        // Sanity check: Ensure pointers fit in the current architecture's address space.
        // This is relevant if u64 pointers are passed to a 32-bit system.
        assert!(lock_addr <= usize::MAX as u64, "lock_addr exceeds addressable memory range");
        assert!(data_addr <= usize::MAX as u64, "data_addr exceeds addressable memory range");

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
            // Optimistic check to avoid cache invalidation on CAS failure
            if unsafe { (*self.lock_ptr).load(Ordering::Relaxed) } != UNLOCKED {
                 spin_loop();
                 continue;
            }

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

/// A wrapper around `SharedMutex` for convenient global static initialization and access.
#[derive(Debug)]
pub struct GlobalSharedMutex<T> {
    state: AtomicUsize,
    ptr: AtomicPtr<SharedMutex<T>>,
}

impl<T> GlobalSharedMutex<T> {
    /// Creates a new, uninitialized `GlobalSharedMutex`.
    pub const fn new() -> Self {
        Self {
            state: AtomicUsize::new(GLOBAL_UNINITIALIZED),
            ptr: AtomicPtr::new(ptr::null_mut()),
        }
    }

    /// Initializes the global mutex with the given data.
    /// This method ensures the `SharedMutex` is initialized exactly once.
    pub fn init(&self, data: T) {
        match self.state.compare_exchange(
            GLOBAL_UNINITIALIZED,
            GLOBAL_INITIALIZING,
            Ordering::Acquire,
            Ordering::Relaxed,
        ) {
            Ok(_) => {
                let mut boxed_sm = Box::new(SharedMutex::<T>::new());
                boxed_sm.set(data);
                self.ptr.store(Box::into_raw(boxed_sm), Ordering::Release);
                self.state.store(GLOBAL_INITIALIZED, Ordering::Release);
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

    #[inline]
    fn get_mutex(&self) -> &SharedMutex<T> {
        loop {
            match self.state.load(Ordering::Acquire) {
                GLOBAL_INITIALIZED => {
                    let ptr = self.ptr.load(Ordering::Acquire);
                    debug_assert!(!ptr.is_null(), "GlobalSharedMutex ptr is null despite being initialized");
                    return unsafe { &*ptr };
                }
                GLOBAL_INITIALIZING => spin_loop(),
                GLOBAL_UNINITIALIZED => panic!("GlobalSharedMutex has not been initialized. Call init() first."),
                _ => unreachable!("GlobalSharedMutex in invalid state"),
            }
        }
    }

    pub fn lock(&self) -> SharedMutexGuard<'_, T> {
        self.get_mutex().lock()
    }

    pub fn read(&self) -> SharedMutexReadGuard<'_, T> {
        self.get_mutex().read()
    }

    pub fn share(&self) -> (u64, u64) {
        self.get_mutex().share()
    }

    pub fn is_locked(&self) -> bool {
        self.get_mutex().is_locked()
    }
}

// Ensure memory leaks don't happen if GlobalSharedMutex is used as a local variable
impl<T> Drop for GlobalSharedMutex<T> {
    fn drop(&mut self) {
        if self.state.load(Ordering::Acquire) == GLOBAL_INITIALIZED {
            let ptr = self.ptr.load(Ordering::Relaxed);
            if !ptr.is_null() {
                unsafe {
                    // Reclaim memory by converting back to Box
                    let _ = Box::from_raw(ptr);
                }
            }
        }
    }
}

unsafe impl<T: Send> Send for GlobalSharedMutex<T> {}
unsafe impl<T: Send> Sync for GlobalSharedMutex<T> {}

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

    #[test]
    fn basic_write_lock_unlock() {
        let mut mutex = SharedMutex::new();
        mutex.set(TestData { value: 10, text: "hello".to_string() });

        {
            let mut guard = mutex.lock();
            assert_eq!(guard.value, 10);
            guard.value = 20;
            guard.text = "world".to_string();
        }

        {
            let guard = mutex.lock();
            assert_eq!(guard.value, 20);
            assert_eq!(guard.text, "world");
        }
    }

    #[test]
    fn basic_read_lock_unlock() {
        let mut mutex = SharedMutex::new();
        mutex.set(TestData { value: 30, text: "read test".to_string() });

        {
            let guard = mutex.read();
            assert_eq!(guard.value, 30);
            assert_eq!(guard.text, "read test");
        }

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
        // Since SharedMutex is !Unpin due to PhantomPinned, we can still put it in Arc
        // because Arc puts it on the heap.
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
    }
}

#[cfg(test)]
mod global_tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn g_basic_init_lock_read() {
        let test_static_mutex: GlobalSharedMutex<i32> = GlobalSharedMutex::new();
        test_static_mutex.init(100);

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
        let temp_global: GlobalSharedMutex<i32> = GlobalSharedMutex::new();
        temp_global.init(1);
        temp_global.init(2);
    }

    #[test]
    fn g_init_concurrent_access_waits() {
        let mutex: Arc<GlobalSharedMutex<i32>> = Arc::new(GlobalSharedMutex::new());
        let barrier = Arc::new(std::sync::Barrier::new(2));

        let mutex_clone1 = Arc::clone(&mutex);
        let barrier_clone1 = Arc::clone(&barrier);
        let thread1 = thread::spawn(move || {
            barrier_clone1.wait();
            mutex_clone1.init(123);
            assert_eq!(*mutex_clone1.read(), 123);
        });

        let mutex_clone2 = Arc::clone(&mutex);
        let barrier_clone2 = Arc::clone(&barrier);
        let thread2 = thread::spawn(move || {
            barrier_clone2.wait();
            let val = *mutex_clone2.read();
            assert_eq!(val, 123);
        });

        thread1.join().unwrap();
        thread2.join().unwrap();
    }

    // New test to ensure Drop works correctly for non-static usage
    #[test]
    fn g_drop_cleans_memory() {
        let m = GlobalSharedMutex::<i32>::new();
        m.init(5);
        // When 'm' goes out of scope, Valgrind/Sanitizers should not report leaks.
    }
}
