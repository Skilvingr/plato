use std::{cell::UnsafeCell, mem::MaybeUninit};

pub struct UnsafeSyncCell<T>(UnsafeCell<MaybeUninit<T>>);

unsafe impl<T> Sync for UnsafeSyncCell<T> {}
unsafe impl<T> Send for UnsafeSyncCell<T> {}

impl<T> UnsafeSyncCell<T> {
    pub fn new(i: T) -> Self {
        Self(UnsafeCell::new(MaybeUninit::new(i)))
    }

    pub fn inner(&self) -> T {
        unsafe { core::mem::replace(&mut *self.0.get(), MaybeUninit::<T>::zeroed()).assume_init() }
    }
}
