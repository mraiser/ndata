/// Thanks and credit to Mikhail Panfilov
/// https://mnwa.medium.com/building-a-stupid-mutex-in-the-rust-d55886538889

use core::sync::atomic::AtomicBool;
use core::sync::atomic::Ordering;
use core::ops::Deref;
use core::ops::DerefMut;
use core::hint::spin_loop;
use core::cell::UnsafeCell;

/// A simple mutex that can be accessed globally. If "mirror" feature is enabled the mutex can be shared across partitions.
#[derive(Debug, Default)]
pub struct SharedMutex<T> {
  /// Indicates whether the mutex is acquired (locked)
  is_acquired_x: AtomicBool,
    /// The underlying object this mutex is locking
  data_x: Option<UnsafeCell<T>>,
  
  my_ia: u64,
  my_d: u64,
}

impl<T> SharedMutex<T> {
  /// Instantiate new mutex with no underlying object to lock
  pub const fn new() -> SharedMutex<T> {
//    println!("NEW");
    SharedMutex {
      is_acquired_x: AtomicBool::new(false),
      data_x: None,
      my_ia: 0,
      my_d: 0,
    }
  }

  /// Set the underlying object to lock
  pub fn set(&mut self, t:T) {
  
    if self.my_ia != 0 { panic!("sharedmutex may only be set once!"); }
    
    self.data_x = Some(UnsafeCell::new(t));
    self.my_ia = (&self.is_acquired_x as *const AtomicBool) as u64;
    self.my_d = (&self.data_x as *const Option<UnsafeCell<T>>) as u64;
  }
    
  /// Share the underlying locked object
  pub fn share(&self) -> (u64, u64) {
    (self.my_ia, self.my_d)
  }
  
  /// Mirror the shared locked object
  pub fn mirror(&mut self, q:u64, r:u64) {
  
    if self.my_ia != 0 { panic!("sharedmutex may not mirror once set!"); }
    
    self.my_ia = q;
    self.my_d = r;
  }

  /// Lock this mutex
  fn do_lock(&self) -> bool {
    unsafe { return (*(self.my_ia as *const AtomicBool)).swap(true, Ordering::AcqRel); }
  }
  
  /// Lock this mutex
  pub fn lock(&self) -> SharedMutexGuard<'_, T> {
    while self.do_lock() {
      spin_loop();
    }
    SharedMutexGuard { mutex: &self }
  }
  
  /// Release the lock on this mutex
  fn release(&self) {
    unsafe { (*(self.my_ia as *const AtomicBool)).store(false, Ordering::Release); }
  }
}

/// Protect the underlying locked object
#[derive(Debug)]
pub struct SharedMutexGuard<'a, T> {
  pub mutex: &'a SharedMutex<T>,
}

/// Get the underlying locked object
impl<T> Deref for SharedMutexGuard<'_, T> {
  type Target = T;
  fn deref(&self) -> &Self::Target {
    unsafe { 
      let b = &mut *(self.mutex.my_d as *mut Option<UnsafeCell<T>>);
      &(*b.as_ref().unwrap().get())
    }
  }
}

/// Get the underlying locked object as mutable
impl<T> DerefMut for SharedMutexGuard<'_, T> {
  fn deref_mut(&mut self) -> &mut Self::Target {
    unsafe { 
      let b = &mut *(self.mutex.my_d as *mut Option<UnsafeCell<T>>);
      &mut (*b.as_mut().unwrap().get())
    }
  }
}

/// Drop the mutex guard
impl<T> Drop for SharedMutexGuard<'_, T> {
  fn drop(&mut self) {
    self.mutex.release()
  }
}

unsafe impl<T> Send for SharedMutex<T> where T: Send {}
unsafe impl<T> Sync for SharedMutex<T> where T: Send {}
unsafe impl<T> Send for SharedMutexGuard<'_, T> where T: Send {}
unsafe impl<T> Sync for SharedMutexGuard<'_, T> where T: Send + Sync {}

