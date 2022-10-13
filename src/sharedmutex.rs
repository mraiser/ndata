// Thanks and credit to Mikhail Panfilov
// https://mnwa.medium.com/building-a-stupid-mutex-in-the-rust-d55886538889

use std::alloc::Layout;
use std::alloc::alloc;
use std::alloc::dealloc;
use std::marker::PhantomData;

use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::ops::Deref;
use std::ops::DerefMut;
use std::hint::spin_loop;
use std::thread::yield_now;

#[cfg(feature="debug_mutex")]
use std::time::Instant;
#[cfg(feature="debug_mutex")]
use backtrace::Backtrace;

#[derive(Debug, Default)]
pub struct SharedMutex<T> {
  is_acquired: u64,
  data: u64,
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
      is_acquired: ptr1 as u64,
      data: ptr2 as u64,
      phantom: PhantomData,
    }
  }

  pub const fn mirror(q:u64, r:u64) -> SharedMutex<T> {
    SharedMutex {
      is_acquired: q,
      data: r,
      phantom: PhantomData,
    }
  }
  
  pub fn share(&self) -> (u64, u64) {
    (self.is_acquired, self.data)
  }
    
  pub fn terminate(&self) {
    unsafe {
      dealloc(self.is_acquired as *mut u8, Layout::new::<AtomicBool>());
      dealloc(self.data as *mut u8, Layout::new::<T>());
    }
  }
    
  pub fn lock(&self) -> SharedMutexGuard<'_, T> {
    #[cfg(feature="debug_mutex")]
    let mut start = Instant::now();
    unsafe { 
      while (*(self.is_acquired as *mut AtomicBool)).swap(true, Ordering::AcqRel) {
        spin_loop();
        yield_now();

        #[cfg(feature="debug_mutex")]
        if start.elapsed().as_secs() > 40 {
          println!("UNUSUALLY LONG WAIT FOR SHAREDMUTEX");
          
          let bt = Backtrace::new();
          println!("{:?}", bt);
          
          println!("UNUSUALLY LONG WAIT FOR SHAREDMUTEX");
          start = Instant::now();
        }
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

