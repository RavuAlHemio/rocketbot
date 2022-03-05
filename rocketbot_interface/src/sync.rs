use std::fmt;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use log::{debug, error};
use tokio;
use tokio::task::JoinError;


pub struct Mutex<T: ?Sized> {
    identifier: &'static str,
    inner_mutex: tokio::sync::Mutex<T>,
}
impl<T: ?Sized> Mutex<T> {
    pub fn new(identifier: &'static str, value: T) -> Self
        where T: Sized
    {
        let inner_mutex = tokio::sync::Mutex::new(value);
        Self {
            identifier,
            inner_mutex,
        }
    }

    pub async fn lock(&self) -> MutexGuard<'_, T> {
        debug!("Mutex: locking {}", self.identifier);
        let inner_guard = self.inner_mutex.lock().await;
        debug!("Mutex: locked {}", self.identifier);
        MutexGuard::new(self.identifier, inner_guard)
    }

    pub fn blocking_lock(&self) -> MutexGuard<'_, T> {
        debug!("Mutex: block-locking {}", self.identifier);
        let inner_guard = self.inner_mutex.blocking_lock();
        debug!("Mutex: block-locked {}", self.identifier);
        MutexGuard::new(self.identifier, inner_guard)
    }
}
impl<T: fmt::Debug> fmt::Debug for Mutex<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Mutex")
            .field("identifier", &self.identifier)
            .field("inner_mutex", &self.inner_mutex)
            .finish()
    }
}

#[derive(Debug)]
pub struct MutexGuard<'a, T: ?Sized> {
    identifier: &'static str,
    inner_guard: tokio::sync::MutexGuard<'a, T>,
}
impl<'a, T: ?Sized> MutexGuard<'a, T> {
    fn new(identifier: &'static str, inner_guard: tokio::sync::MutexGuard<'a, T>) -> Self {
        Self {
            identifier,
            inner_guard,
        }
    }
}
impl<T: ?Sized> Deref for MutexGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.inner_guard.deref()
    }
}
impl<T: ?Sized> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner_guard.deref_mut()
    }
}
impl<T: ?Sized> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        debug!("Mutex: unlocking {}", self.identifier);
    }
}


pub struct RwLock<T: ?Sized> {
    identifier: &'static str,
    inner_lock: tokio::sync::RwLock<T>,
}
impl<T: ?Sized> RwLock<T> {
    pub fn new(identifier: &'static str, value: T) -> Self
        where T: Sized
    {
        let inner_lock = tokio::sync::RwLock::new(value);
        Self {
            identifier,
            inner_lock,
        }
    }

    pub async fn read(&self) -> RwLockReadGuard<'_, T> {
        debug!("RwLock: read-locking {}", self.identifier);
        let inner_guard = self.inner_lock.read().await;
        debug!("RwLock: read-locked {}", self.identifier);
        RwLockReadGuard::new(self.identifier, inner_guard)
    }

    pub async fn write(&self) -> RwLockWriteGuard<'_, T> {
        debug!("RwLock: write-locking {}", self.identifier);
        let inner_guard = self.inner_lock.write().await;
        debug!("RwLock: write-locked {}", self.identifier);
        RwLockWriteGuard::new(self.identifier, inner_guard)
    }
}
impl<T: fmt::Debug> fmt::Debug for RwLock<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RwLock")
            .field("identifier", &self.identifier)
            .field("inner_lock", &self.inner_lock)
            .finish()
    }
}

pub struct RwLockReadGuard<'a, T: ?Sized> {
    identifier: &'static str,
    inner_guard: tokio::sync::RwLockReadGuard<'a, T>,
}
impl<'a, T: ?Sized> RwLockReadGuard<'a, T> {
    fn new(identifier: &'static str, inner_guard: tokio::sync::RwLockReadGuard<'a, T>) -> Self {
        Self {
            identifier,
            inner_guard,
        }
    }
}
impl<T: ?Sized> Deref for RwLockReadGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.inner_guard.deref()
    }
}
impl<T: ?Sized> Drop for RwLockReadGuard<'_, T> {
    fn drop(&mut self) {
        debug!("RwLock: read-unlocking {}", self.identifier);
    }
}

pub struct RwLockWriteGuard<'a, T: ?Sized> {
    identifier: &'static str,
    inner_guard: tokio::sync::RwLockWriteGuard<'a, T>,
}
impl<'a, T: ?Sized> RwLockWriteGuard<'a, T> {
    fn new(identifier: &'static str, inner_guard: tokio::sync::RwLockWriteGuard<'a, T>) -> Self {
        Self {
            identifier,
            inner_guard,
        }
    }
}
impl<T: ?Sized> Deref for RwLockWriteGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.inner_guard.deref()
    }
}
impl<T: ?Sized> DerefMut for RwLockWriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner_guard.deref_mut()
    }
}
impl<T: ?Sized> Drop for RwLockWriteGuard<'_, T> {
    fn drop(&mut self) {
        debug!("RwLock: write-unlocking {}", self.identifier);
    }
}


#[derive(Debug)]
pub enum StoppableTaskResult<R> {
    Success(R),
    Timeout,
    TaskPanicked(JoinError),
    ChannelBreakdown,
}


/// Spawns a blocking task that takes an [`AtomicBool`] as a stop-early flag, raises the flag if a
/// timeout elapses, and collects the task's result.
///
/// The task is expected to return [`None`] if it was stopped early and [`Some(_)`] if it has
/// completed successfully. The [`AtomicBool`] is initially `false` and is changed to `true` once
/// the given timeout elapses.
pub async fn run_stoppable_task_timeout<F, R>(timeout: Duration, task: F) -> StoppableTaskResult<R>
        where F: FnOnce(&AtomicBool) -> Option<R> + Send + 'static, R: Send + 'static {
    let stopper = Arc::new(AtomicBool::new(false));
    let task_stopper = Arc::clone(&stopper);

    // we need to use a channel here because oneshot etc. are consumed
    // the first time they are awaited, which doesn't work well with tokio::select!
    let (result_send, mut result_receive) = tokio::sync::mpsc::channel(1);

    let factors_handle = tokio::task::spawn_blocking(move || {
        let result = task(&task_stopper);
        if let Err(e) = result_send.blocking_send(result) {
            error!("sending result failed: {}", e);
        }
    });
    let sleepyhead = tokio::time::sleep(timeout);
    let result_opt_opt = tokio::select! {
        _ = sleepyhead => {
            // signal end
            stopper.store(true, Ordering::SeqCst);

            // wait for finish
            result_receive.recv().await
        },
        factors_opt = result_receive.recv() => {
            // done
            factors_opt
        },
    };
    if let Err(e) = factors_handle.await {
        return StoppableTaskResult::TaskPanicked(e);
    }

    match result_opt_opt {
        None => StoppableTaskResult::ChannelBreakdown,
        Some(None) => StoppableTaskResult::Timeout,
        Some(Some(r)) => StoppableTaskResult::Success(r),
    }
}
