// Thanks and credit to Mikhail Panfilov
// https://mnwa.medium.com/building-a-stupid-mutex-in-the-rust-d55886538889

use std::alloc::Layout;
use std::alloc::alloc;
use std::marker::PhantomData;

use std::sync::atomic::{AtomicBool, Ordering};
use std::ops::{Deref, DerefMut};
use std::hint::spin_loop;
use std::thread::yield_now;

#[derive(Debug, Default)]
pub struct SharedMutex<T> {
  pub is_acquired: usize,
  data: usize,
  phantom: PhantomData<T>,
}

impl<T> SharedMutex<T> {
  pub fn new() -> SharedMutex<T> {
    let ptr1;
    let ptr2;
    unsafe {
      let layout = Layout::new::<AtomicBool>();
      ptr1 = alloc(layout);
      let layout = Layout::new::<T>();
      ptr2 = alloc(layout);
    }
      
    SharedMutex {
      is_acquired: ptr1 as usize,
      data: ptr2 as usize,
      phantom: PhantomData,
    }
  }

  pub const fn mirror(q:usize, r:usize) -> SharedMutex<T> {
    SharedMutex {
      is_acquired: q,
      data: r,
      phantom: PhantomData,
    }
  }
  
  pub fn share(&self) -> (usize, usize) {
    (self.is_acquired, self.data)
  }
    
  pub fn lock(&self) -> SharedMutexGuard<'_, T> {
    unsafe { 
      while (*(self.is_acquired as *mut AtomicBool)).swap(true, Ordering::AcqRel) {
        spin_loop();
        yield_now();
      }
    }
    SharedMutexGuard { mutex: &self }
  }
  
  fn release(&self) {
    unsafe { (*(self.is_acquired as *mut AtomicBool)).store(false, Ordering::Release); }
  }
}

#[derive(Debug)]
pub struct SharedMutexGuard<'a, T> {
  pub mutex: &'a SharedMutex<T>,
}

impl<T> Deref for SharedMutexGuard<'_, T> {
  type Target = T;
  fn deref(&self) -> &Self::Target {
    unsafe { 
      let b = &mut *(self.mutex.data as *mut T);
      &(*b) 
    }
  }
}

impl<T> DerefMut for SharedMutexGuard<'_, T> {
  fn deref_mut(&mut self) -> &mut Self::Target {
    unsafe { 
      let b = &mut *(self.mutex.data as *mut T);
      &mut (*b) 
    }
  }
}

impl<T> Drop for SharedMutexGuard<'_, T> {
  fn drop(&mut self) {
    self.mutex.release()
  }
}

unsafe impl<T> Send for SharedMutex<T> where T: Send {}
unsafe impl<T> Sync for SharedMutex<T> where T: Send {}
unsafe impl<T> Send for SharedMutexGuard<'_, T> where T: Send {}
unsafe impl<T> Sync for SharedMutexGuard<'_, T> where T: Send + Sync {}

