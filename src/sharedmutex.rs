// Thanks and credit to Mikhail Panfilov
// https://mnwa.medium.com/building-a-stupid-mutex-in-the-rust-d55886538889

use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::ops::Deref;
use std::ops::DerefMut;
use std::hint::spin_loop;
use std::thread::yield_now;

#[cfg(not(feature="reload"))]
use std::cell::UnsafeCell;

#[cfg(feature="reload")]
use std::alloc::Layout;
#[cfg(feature="reload")]
use std::alloc::alloc;
#[cfg(feature="reload")]
use std::alloc::dealloc;
#[cfg(feature="reload")]
use std::marker::PhantomData;

#[cfg(feature="debug_mutex")]
use std::time::Instant;
#[cfg(feature="debug_mutex")]
use backtrace::Backtrace;

#[derive(Debug, Default)]
pub struct SharedMutex<T> {
  #[cfg(not(feature="reload"))]
  is_acquired: AtomicBool,
  #[cfg(not(feature="reload"))]
  data: Option<UnsafeCell<T>>,
  
  #[cfg(feature="reload")]
  is_acquired: u64,
  #[cfg(feature="reload")]
  data: u64,
  #[cfg(feature="reload")]
  phantom: PhantomData<T>,
}

impl<T> SharedMutex<T> {
  pub const fn new() -> SharedMutex<T> {
    SharedMutex {
      #[cfg(not(feature="reload"))]
      is_acquired: AtomicBool::new(false),
      #[cfg(not(feature="reload"))]
      data: None,
      
      #[cfg(feature="reload")]
      is_acquired: 0,
      #[cfg(feature="reload")]
      data: 0,
      #[cfg(feature="reload")]
      phantom: PhantomData,
    }
  }
  
  #[cfg(not(feature="reload"))]
  pub fn set(&mut self, t:T) {
    self.data = Some(UnsafeCell::new(t));
  }
  
  #[cfg(feature="reload")]
  pub fn init(&mut self) {
    unsafe {
      let layout = Layout::new::<AtomicBool>();
      self.is_acquired = alloc(layout) as u64;
      let layout = Layout::new::<T>();
      self.data = alloc(layout)as u64;
    }
  }
  
  #[cfg(feature="reload")]
  pub const fn mirror(q:u64, r:u64) -> SharedMutex<T> {
    SharedMutex {
      is_acquired: q,
      data: r,
      phantom: PhantomData,
    }
  }
  
  #[cfg(feature="reload")]
  pub fn share(&self) -> (u64, u64) {
    (self.is_acquired, self.data)
  }
  
  #[cfg(feature="reload")]
  pub fn terminate(&self) {
    unsafe {
      dealloc(self.is_acquired as *mut u8, Layout::new::<AtomicBool>());
      dealloc(self.data as *mut u8, Layout::new::<T>());
    }
  }
    
  fn do_lock(&self) -> bool {
    #[cfg(feature="reload")]
    unsafe { return (*(self.is_acquired as *mut AtomicBool)).swap(true, Ordering::AcqRel); }
    #[cfg(not(feature="reload"))]
    self.is_acquired.swap(true, Ordering::AcqRel)
  }
  
  pub fn lock(&self) -> SharedMutexGuard<'_, T> {
    #[cfg(feature="debug_mutex")]
    let mut start = Instant::now();
    while self.do_lock() {
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
    SharedMutexGuard { mutex: &self }
  }
  
  fn release(&self) {
    #[cfg(feature="reload")]
    unsafe { (*(self.is_acquired as *mut AtomicBool)).store(false, Ordering::Release); }
    #[cfg(not(feature="reload"))]
    self.is_acquired.store(false, Ordering::Release);
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
      #[cfg(feature="reload")]
      let b = &mut *(self.mutex.data as *mut T);
      #[cfg(not(feature="reload"))]
      let b = &mut *(self.mutex.data.as_ref().unwrap().get() as *mut T);
      &(*b) 
    }
  }
}

impl<T> DerefMut for SharedMutexGuard<'_, T> {
  fn deref_mut(&mut self) -> &mut Self::Target {
    unsafe { 
      #[cfg(feature="reload")]
      let b = &mut *(self.mutex.data as *mut T);
      #[cfg(not(feature="reload"))]
      let b = &mut *(self.mutex.data.as_ref().unwrap().get() as *mut T);
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

