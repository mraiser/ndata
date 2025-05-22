//! A simple, shareable spinlock mutex implementation.
//! Credit for the original spinlock logic: Mikhail Panfilov
//! https://mnwa.medium.com/building-a-stupid-mutex-in-the-rust-d55886538889

use core::cell::UnsafeCell;
use core::hint::spin_loop;
use core::ops::{Deref, DerefMut};
use core::ptr;
use core::sync::atomic::{AtomicBool, Ordering};

// Define constants for pointer types for clarity
type LockPtr = *const AtomicBool;
// Pointer to the UnsafeCell containing the data T
type DataPtr<T> = *const UnsafeCell<T>;

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

/// A simple spinlock mutex that can potentially be shared across memory partitions using raw pointers.
///
/// This mutex uses atomic operations for locking and `UnsafeCell` for interior mutability.
/// The `share` and `mirror` methods allow creating mutex instances that point to the
/// same underlying lock flag and data, potentially across different memory spaces,
/// but this requires careful handling due to the use of raw pointers.
///
/// # Safety
///
/// This mutex relies on raw pointers for its "mirroring" functionality (`share` and `mirror` methods).
/// This is **inherently unsafe** and imposes strict requirements on the user:
///
/// 1.  **Non-Movement:** The `SharedMutex` instance that calls `set` (the "original" instance)
///     **must not be moved** in memory after `set` is called and while any mirrored instances exist.
///     Moving the original instance will invalidate the pointers shared via `share`, leading to
///     dangling pointers and **undefined behavior** in mirrored instances. Placing the original
///     `SharedMutex` in a `static` variable is the safest way to ensure this non-movement requirement.
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
///     operations (like `lock` or `share`). This mutex does not provide internal synchronization
///     for its own initialization.
///
/// **Failure to meet these conditions will result in undefined behavior.** Use the `share`
/// and `mirror` features with extreme caution and only when the safety requirements can be
/// strictly guaranteed. For standard concurrent programming within a single process, prefer
/// `std::sync::Mutex` or other safer abstractions.
#[derive(Debug)]
pub struct SharedMutex<T> {
  /// Pointer to the atomic lock flag (`AtomicBool`).
  /// - If `state == MutexState::Local`, this points to `local_lock_storage`.
  /// - If `state == MutexState::Mirrored`, this points to the lock of the original mutex.
  /// - If `state == MutexState::Uninitialized`, this pointer is invalid (likely null).
  lock_ptr: LockPtr,

  /// Pointer to the `UnsafeCell` containing the data `T`.
  /// - If `state == MutexState::Local`, this points to the `UnsafeCell` inside `local_data_storage`.
  /// - If `state == MutexState::Mirrored`, this points to the data cell of the original mutex.
  /// - If `state == MutexState::Uninitialized`, this pointer is invalid (likely null).
  data_ptr: DataPtr<T>,

  /// Tracks whether the mutex is local, mirrored, or uninitialized.
  state: MutexState,

  // --- Fields used only when state == MutexState::Local ---
  // These fields store the actual lock and data *only* when the mutex is the original owner.
  // They must not be moved after `set()` if the mutex is shared.

  /// The storage for the lock flag when the mutex is `Local`.
  local_lock_storage: AtomicBool,

  /// The storage for the data (`T`) wrapped in `UnsafeCell` when the mutex is `Local`.
  /// `Option` allows delayed initialization via `set`.
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
  ///
  /// The mutex is not usable until it is initialized by calling either `set` (to make it
  /// the owner of new data) or `mirror` (to make it point to another mutex's data).
  #[inline]
  pub const fn new() -> SharedMutex<T> {
    SharedMutex {
      // Pointers are initially invalid and must be set by `set` or `mirror`.
      // Using null is a conventional way to represent an invalid/uninitialized pointer.
      lock_ptr: ptr::null(),
      data_ptr: ptr::null(),
      state: MutexState::Uninitialized,
      // Initialize local storage, though it's only relevant after `set` is called.
      local_lock_storage: AtomicBool::new(false), // false = unlocked
      local_data_storage: None,
    }
  }

  /// Initializes the mutex with the given data `t`, making it a "local" mutex
  /// that owns and manages its own lock and data.
  ///
  /// This method takes ownership of the data `t`.
  ///
  /// # Panics
  ///
  /// Panics if the mutex has already been initialized (either by a previous call
  /// to `set` or by a call to `mirror`). A `SharedMutex` can only be initialized once.
  ///
  /// # Safety Note on Sharing
  ///
  /// If you intend to call `share()` on this mutex later, the `SharedMutex` instance
  /// **must not move** in memory after this `set` call completes. Moving it would
  /// invalidate the internal pointers (`lock_ptr`, `data_ptr`) that `share` returns.
  /// See the main `SharedMutex` safety documentation for details.
  pub fn set(&mut self, t: T) {
    // Ensure this mutex hasn't been initialized already.
    if self.state != MutexState::Uninitialized {
      panic!("SharedMutex may only be initialized once (using set or mirror)");
    }

    // Store the data locally, wrapped in UnsafeCell for interior mutability.
    self.local_data_storage = Some(UnsafeCell::new(t));
    // Ensure the local lock starts in the 'unlocked' state (false).
    // Note: It's initialized to false in `new`, but setting it again here ensures
    // correctness if `new` were changed.
    self.local_lock_storage.store(false, Ordering::Relaxed); // Relaxed is fine for initialization state.

    // Set the internal pointers to point to the local storage fields.
    // SAFETY: We are getting pointers to fields within `self`. This is safe *only if*
    // `self` does not move after this point and before any mirrored pointers derived
    // from `share()` are used. This is a critical safety requirement for sharing.
    self.lock_ptr = &self.local_lock_storage as *const AtomicBool;

    // Get a pointer to the UnsafeCell<T> inside the Option.
    // SAFETY: We just set local_data_storage to Some(..), so `as_ref().unwrap()` is safe.
    // Casting the resulting `&UnsafeCell<T>` to `*const UnsafeCell<T>` is safe.
    self.data_ptr = self.local_data_storage.as_ref().unwrap() as *const UnsafeCell<T>;

    // Mark the state as Local, indicating it now owns the data.
    self.state = MutexState::Local;
  }

  /// Returns the raw memory addresses of the lock flag and the data cell as `u64` integers.
  ///
  /// These addresses can be passed to another `SharedMutex` instance's `mirror` method,
  /// allowing the second instance to access the same underlying lock and data. This is
  /// the basis for the cross-partition sharing mechanism.
  ///
  /// # Panics
  ///
  /// Panics if the mutex has not been initialized using `set`. A mirrored mutex (`state == Mirrored`)
  /// or an uninitialized mutex cannot provide addresses to share.
  ///
  /// # Safety
  ///
  /// The returned `u64` values represent raw pointers. Their validity depends entirely on the
  /// original `SharedMutex` instance (the one `share` was called on):
  /// - It **must not move** in memory after `set` was called.
  /// - It must remain alive (not be dropped) for as long as the returned addresses are used.
  ///
  /// Using these addresses after the original mutex moves or is dropped leads to
  /// **undefined behavior**. See the main `SharedMutex` safety documentation.
  pub fn share(&self) -> (u64, u64) {
    // Can only share a mutex that has been initialized locally via `set`.
    if self.state != MutexState::Local {
      panic!("Only a locally set SharedMutex can be shared (must be initialized with `set`)");
    }
    // Internal consistency check: Pointers must be valid if state is Local.
    debug_assert!(!self.lock_ptr.is_null(), "Internal error: null lock_ptr in Local state");
    debug_assert!(!self.data_ptr.is_null(), "Internal error: null data_ptr in Local state");

    // Cast the raw pointers to u64 for transport across potential boundaries.
    // This relies on the pointer-to-integer cast being valid and reversible.
    (self.lock_ptr as u64, self.data_ptr as u64)
  }

  /// Initializes this mutex to mirror another `SharedMutex` using the provided raw memory addresses.
  ///
  /// The `lock_addr` and `data_addr` arguments should be the `u64` values obtained from the
  /// `share` method of another `SharedMutex` instance (the "original" instance). This instance
  /// will then use the lock and data located at those addresses.
  ///
  /// # Panics
  ///
  /// Panics if this mutex has already been initialized (either by a previous call
  /// to `set` or by a call to `mirror`). A `SharedMutex` can only be initialized once.
  /// Panics if either `lock_addr` or `data_addr` is zero (null pointer).
  ///
  /// # Safety
  ///
  /// This is an **inherently unsafe** operation. The caller **must guarantee** that:
  /// - `lock_addr` and `data_addr` are valid addresses obtained from a `share()` call
  ///   on a live, non-moved `SharedMutex<T>` instance (the original).
  /// - The original `SharedMutex<T>` remains valid (alive and unmoved) for the
  ///   entire lifetime of this mirrored instance.
  /// - The memory at these addresses is accessible and correctly interpreted in the
  ///   current context.
  ///
  /// Providing invalid addresses or using this mirrored mutex after the original is
  /// invalidated (moved or dropped) leads to **undefined behavior**.
  pub fn mirror(&mut self, lock_addr: u64, data_addr: u64) {
    // Ensure this mutex hasn't been initialized already.
    if self.state != MutexState::Uninitialized {
      panic!("SharedMutex may only be initialized once (using set or mirror)");
    }

    // Basic sanity check: Ensure provided addresses are not null.
    if lock_addr == 0 || data_addr == 0 {
      panic!("Cannot mirror using null addresses (lock_addr={}, data_addr={})", lock_addr, data_addr);
    }

    // Convert u64 addresses back to raw pointers.
    // SAFETY: This is the core unsafe part of mirroring. The validity of the
    // resulting pointers depends entirely on the caller upholding the safety contract
    // documented above and in the main struct docs. We trust the caller provided valid addresses.
    self.lock_ptr = lock_addr as LockPtr;
    self.data_ptr = data_addr as DataPtr<T>;

    // Mark the state as Mirrored. Local storage fields will not be used.
    self.state = MutexState::Mirrored;
    // Explicitly set local data to None to reflect it's not used in mirrored state.
    self.local_data_storage = None;
  }

  /// Acquires the lock, spinning (busy-waiting) until it becomes available.
  ///
  /// Returns a `SharedMutexGuard` instance. The lock is held while the guard exists,
  /// and automatically released when the guard is dropped.
  ///
  /// # Panics
  ///
  /// Panics if the mutex has not been initialized (i.e., if `state` is `Uninitialized`).
  /// You must call `set` or `mirror` before attempting to lock the mutex.
  #[inline]
  pub fn lock(&self) -> SharedMutexGuard<'_, T> {
    // Ensure the mutex is initialized before trying to lock.
    if !self.is_initialized() { // Use the new helper method
      panic!("Cannot lock an uninitialized SharedMutex (call `set` or `mirror` first)");
    }
    // Internal consistency check: Pointers must be valid if initialized.
    debug_assert!(!self.lock_ptr.is_null(), "Internal error: null lock_ptr in lock()");
    debug_assert!(!self.data_ptr.is_null(), "Internal error: null data_ptr in lock()");


    // Spin until the lock is acquired.
    // SAFETY: Accessing the memory pointed to by `lock_ptr` relies on the safety contract
    // of `set` (for Local state) or `mirror` (for Mirrored state) ensuring `lock_ptr`
    // points to a valid `AtomicBool`.
    // The `swap` operation atomically attempts to set the flag to `true` (locked) and
    // returns the *previous* value. The loop continues as long as the previous value was `true`.
    // `Ordering::AcqRel` (Acquire-Release) is used:
    // - `Acquire` semantics: Ensures that memory operations after acquiring the lock are not
    //   reordered before the lock acquisition. Makes prior writes by other threads visible.
    // - `Release` semantics: Ensures that memory operations before acquiring the lock are not
    //   reordered after it. (Needed for `swap` which performs both read and write).
    // This combination ensures mutual exclusion and visibility of data protected by the lock.
    while unsafe { (*self.lock_ptr).swap(true, Ordering::AcqRel) } {
      // Yield hint to the CPU that we are in a busy-wait loop.
      spin_loop();
    }

    // Lock successfully acquired, return the guard object.
    SharedMutexGuard { mutex: self }
  }

  /// Releases the lock held by the current thread.
  ///
  /// This method is called implicitly when a `SharedMutexGuard` is dropped.
  /// It should not typically be called directly.
  ///
  /// # Safety (Internal Method)
  ///
  /// This method assumes:
  /// 1. The mutex is initialized (`state` is `Local` or `Mirrored`).
  /// 2. `lock_ptr` points to a valid `AtomicBool`.
  /// 3. The current thread actually holds the lock.
  /// Calling this without holding the lock can lead to incorrect lock state.
  #[inline]
  fn release(&self) {
    // Check if initialized (should always be true if called from a guard).
    // Use debug_assert for internal consistency checks that should ideally be unreachable in release builds.
    debug_assert!(self.is_initialized(), "Attempted to release an uninitialized mutex");
    debug_assert!(!self.lock_ptr.is_null(), "Attempted to release with a null lock_ptr");

    // SAFETY: Accessing `lock_ptr` relies on the safety contract (pointer must be valid).
    // We assume `lock_ptr` points to a valid `AtomicBool`.
    // `Ordering::Release` ensures that all writes to the protected data by the current
    // thread happen *before* the lock is released. This makes those writes visible to
    // the next thread that acquires the lock (using `Acquire` or `AcqRel`).
    unsafe {
      (*self.lock_ptr).store(false, Ordering::Release); // false = unlocked
    }
  }

  /// Checks if the mutex is currently locked (i.e., if the lock flag is set).
  ///
  /// Note: The result represents the state at a single moment in time. In concurrent
  /// scenarios, the lock state might change immediately after this check. This method
  /// is primarily useful for debugging, assertions, or specific synchronization patterns
  /// where a momentary check is sufficient. It should not be used for general-purpose
  /// lock-free logic based on its return value, as that can lead to race conditions.
  ///
  /// # Panics
  /// Panics if the mutex is uninitialized (`state == Uninitialized`).
  #[inline]
  pub fn is_locked(&self) -> bool {
    if !self.is_initialized() { // Use the new helper method
      panic!("Cannot check lock status of an uninitialized SharedMutex");
    }
    debug_assert!(!self.lock_ptr.is_null(), "Internal error: null lock_ptr in is_locked()");
    // SAFETY: Accessing lock_ptr relies on the safety contract (pointer must be valid).
    // `Ordering::Acquire` ensures that if this read sees `true` (locked), any subsequent
    // reads by this thread will not be reordered before this check. If used in conditions
    // like `if !mutex.is_locked() { /* do something */ }`, Acquire provides some ordering,
    // though such patterns are often racy without further synchronization.
    unsafe { (*self.lock_ptr).load(Ordering::Acquire) }
  }

  /// Checks if the mutex has been initialized via `set` or `mirror`.
  ///
  /// Returns `true` if the state is `Local` or `Mirrored`, `false` otherwise (`Uninitialized`).
  #[inline]
  pub fn is_initialized(&self) -> bool {
    self.state != MutexState::Uninitialized
  }
}

/// A guard that provides scoped access to the data protected by a `SharedMutex`.
///
/// When an instance of `SharedMutexGuard` is created via `SharedMutex::lock()`, it indicates
/// that the lock has been acquired. The guard provides access (immutable via `Deref`, mutable
/// via `DerefMut`) to the data protected by the mutex.
///
/// When the `SharedMutexGuard` goes out of scope (is dropped), the lock on the `SharedMutex`
/// is automatically released. This RAII (Resource Acquisition Is Initialization) pattern
/// ensures that locks are always released, preventing deadlocks caused by forgetting to unlock.
#[derive(Debug)]
#[must_use = "if unused the Mutex will immediately unlock"] // Lint encourages using the guard
pub struct SharedMutexGuard<'a, T> {
  // Holds a reference to the SharedMutex instance that was locked.
  // This reference ensures the mutex stays alive while the guard exists
  // and allows the guard to call `mutex.release()` on drop.
  mutex: &'a SharedMutex<T>,
}

impl<T> Deref for SharedMutexGuard<'_, T> {
  type Target = T;

  /// Provides immutable access (`&T`) to the data protected by the mutex.
  ///
  /// This method allows reading the data while the lock is held.
  #[inline]
  fn deref(&self) -> &Self::Target {
    // SAFETY: This unsafe block is justified because:
    // 1. Lock Acquisition: A `SharedMutexGuard` instance only exists if the corresponding
    //    `SharedMutex::lock()` call successfully acquired the lock, ensuring exclusive
    //    or shared (but currently exclusive for write, safe for read) access.
    // 2. Pointer Validity: `self.mutex.data_ptr` is assumed to be valid due to the
    //    safety contract of `SharedMutex::set` or `SharedMutex::mirror`. It points
    //    to the `UnsafeCell<T>` containing the data.
    // 3. `UnsafeCell::get()`: This method returns a raw pointer (`*mut T`) to the data
    //    inside the `UnsafeCell`. This is the mechanism `UnsafeCell` provides to bypass
    //    Rust's compile-time borrowing rules.
    // 4. Dereferencing to `&T`: Casting the `*mut T` to `&T` (shared/immutable reference)
    //    is safe here because the lock prevents concurrent *writes*. Multiple concurrent
    //    reads (if other guards existed, though this spinlock ensures exclusivity)
    //    would be safe. The lock guarantees no data races for immutable access.
    // 5. Lifetime: The lifetime of the returned reference (`&Self::Target`) is tied
    //    to the lifetime of the guard (`'_`), ensuring the lock is held for the
    //    duration the reference is valid.
    unsafe {
      // Assertions for internal consistency (should be optimized out in release)
      debug_assert!(self.mutex.is_initialized(), "Guard exists for uninitialized mutex");
      debug_assert!(!self.mutex.data_ptr.is_null(), "Guard exists with null data_ptr");
      // Get the raw pointer from UnsafeCell and dereference it to a shared reference.
      &*(*self.mutex.data_ptr).get()
    }
  }
}

impl<T> DerefMut for SharedMutexGuard<'_, T> {
  /// Provides mutable access (`&mut T`) to the data protected by the mutex.
  ///
  /// This method allows reading and writing the data while the lock is held.
  #[inline]
  fn deref_mut(&mut self) -> &mut Self::Target {
    // SAFETY: This unsafe block is justified because:
    // 1. Lock Acquisition: The existence of the guard guarantees the lock was acquired.
    //    Spinlocks like this one ensure *exclusive* access when locked.
    // 2. Pointer Validity: `self.mutex.data_ptr` is assumed valid per the `SharedMutex`
    //    safety contract, pointing to the `UnsafeCell<T>`.
    // 3. `UnsafeCell::get()`: Returns a raw pointer (`*mut T`) to the data.
    // 4. Dereferencing to `&mut T`: Casting `*mut T` to `&mut T` (mutable reference)
    //    is safe here because the lock guarantees *exclusive* access. No other thread
    //    can be reading or writing the data concurrently. This prevents data races.
    // 5. Lifetime: The lifetime of the returned mutable reference is tied to the guard's
    //    lifetime, ensuring the lock remains held while the mutable reference exists.
    unsafe {
      // Assertions for internal consistency
      debug_assert!(self.mutex.is_initialized(), "Guard exists for uninitialized mutex");
      debug_assert!(!self.mutex.data_ptr.is_null(), "Guard exists with null data_ptr");
      // Get the raw pointer from UnsafeCell and dereference it to a mutable reference.
      &mut *(*self.mutex.data_ptr).get()
    }
  }
}

impl<T> Drop for SharedMutexGuard<'_, T> {
  /// Releases the lock when the guard goes out of scope.
  ///
  /// This is automatically called by the Rust compiler when the guard variable's
  /// lifetime ends, ensuring the mutex is unlocked (RAII).
  #[inline]
  fn drop(&mut self) {
    // Check if the mutex is initialized before trying to release.
    // This prevents potential issues if a guard somehow exists for an uninitialized mutex,
    // although the lock() method should prevent this scenario.
    if self.mutex.is_initialized() {
      self.mutex.release();
    }
    // If not initialized, there's nothing to release.
  }
}

// =============================================================================
// Send/Sync Implementations
// =============================================================================
// These unsafe implementations declare that SharedMutex and SharedMutexGuard
// are safe to send between threads (Send) or share references between threads (Sync)
// under certain conditions, despite containing raw pointers and UnsafeCell.
// The safety relies heavily on the correct usage of the mutex (especially the
// share/mirror contract) and the atomic operations ensuring synchronization.

// SAFETY justification for `unsafe impl<T: Send> Send for SharedMutex<T>`:
// A `SharedMutex<T>` can be sent to another thread if the type `T` it protects is `Send`.
// - The `AtomicBool` (pointed to by `lock_ptr` or stored in `local_lock_storage`) is `Send`.
// - The `UnsafeCell<T>` (pointed to by `data_ptr` or stored in `local_data_storage`) contains `T`.
//   If `T` is `Send`, the `UnsafeCell<T>` can be sent.
// - The raw pointers (`lock_ptr`, `data_ptr`) themselves can be sent.
// Crucially, the *validity* of these pointers in the destination thread depends on the
// `share`/`mirror` safety contract (e.g., pointing to static or otherwise valid memory).
// Assuming the contract is upheld, sending the mutex structure itself is safe if `T: Send`.
unsafe impl<T: Send> Send for SharedMutex<T> {}

// SAFETY justification for `unsafe impl<T: Send> Sync for SharedMutex<T>`:
// A `&SharedMutex<T>` (a shared reference to the mutex) can be shared across threads
// (allowing multiple threads to call `lock()` concurrently on the *same* mutex instance)
// if `T` is `Send`.
// - Access to the lock state (`AtomicBool` via `lock_ptr`) is synchronized using atomic operations
//   (`swap`, `store`, `load` with appropriate `Ordering`), making concurrent calls to `lock`/`release` safe.
// - Access to the data (`UnsafeCell<T>` via `data_ptr`) is protected by the lock. The `lock()` method
//   ensures only one thread can obtain a `SharedMutexGuard` at a time.
// - The `SharedMutexGuard` uses `UnsafeCell::get()` to access the data, but the lock guarantees
//   that these accesses do not cause data races.
// - For `&UnsafeCell<T>` to be `Sync`, the inner type `T` must be `Send`. This is because
//   even if the `UnsafeCell` itself isn't mutated directly via the shared reference, the `get()`
//   method allows creating `*mut T`, and operations through that pointer could potentially send `T`
//   values across threads if not properly synchronized. The mutex provides this synchronization.
// Therefore, if `T: Send`, `SharedMutex<T>` is `Sync`.
unsafe impl<T: Send> Sync for SharedMutex<T> {}


// --- Send/Sync for SharedMutexGuard ---
// These follow standard patterns for guards: a guard is Send/Sync if the mutex
// it refers to is Sync and the data T meets certain bounds.

// SAFETY justification for `unsafe impl<'a, T: Send> Send for SharedMutexGuard<'a, T>`:
// A `SharedMutexGuard<'a, T>` contains `&'a SharedMutex<T>`.
// The guard can be sent to another thread if:
// 1. The reference `&'a SharedMutex<T>` can be sent. This requires `SharedMutex<T>` to be `Sync`.
//    We established `SharedMutex<T>` is `Sync` if `T: Send`.
// 2. The operations performed by the guard on `T` are safe when the guard is sent. Access to `T`
//    happens via `deref`/`deref_mut`. Sending the guard essentially transfers the exclusive lock
//    ownership. If `T` itself is `Send`, transferring this ownership is safe.
// Thus, `SharedMutexGuard` is `Send` if `T` is `Send`.
unsafe impl<'a, T: Send> Send for SharedMutexGuard<'a, T> {}

// SAFETY justification for `unsafe impl<'a, T: Send + Sync> Sync for SharedMutexGuard<'a, T>`:
// A `&SharedMutexGuard<'a, T>` (a shared reference to the guard) can be shared across threads if:
// 1. The reference `&'a SharedMutex<T>` inside the guard is `Sync`. This requires `SharedMutex<T>`
//    to be `Sync`, which holds if `T: Send`.
// 2. The operations accessible via `&SharedMutexGuard` are thread-safe. The primary operation is
//    `deref(&self)`, which provides `&T`. For `&T` obtained via the guard to be safely shared
//    across threads (`Sync`), the type `T` itself must be `Sync`.
// Note: `deref_mut(&mut self)` requires `&mut SharedMutexGuard`, so it doesn't affect the `Sync` impl.
// Thus, `SharedMutexGuard` is `Sync` if `T` is `Send + Sync`.
unsafe impl<'a, T: Send + Sync> Sync for SharedMutexGuard<'a, T> {}
