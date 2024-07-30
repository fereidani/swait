#![doc = include_str!("../README.md")]

use std::{
    future::Future,
    pin::*,
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    },
    task::*,
    thread::{yield_now, Thread},
};

thread_local! {
    // A reusable signal instance per thread.
    static THREAD_SIGNAL: Arc<Signal> = Arc::new(Signal::new());
}

/// Extension trait for blocking on a future.
pub trait FutureExt: Future {
    /// Blocks the current thread until the future is ready.
    ///
    /// # Example
    ///
    /// ```
    /// use swait::FutureExt;
    /// let my_fut = async {};
    /// let result = my_fut.swait();
    /// ```
    #[inline(always)]
    fn swait(self) -> Self::Output
    where
        Self: Sized,
    {
        swait(self)
    }
}

impl<F: Future> FutureExt for F {}

const WAITING: u8 = 0;
const PARKED: u8 = 1;
const NOTIFIED: u8 = 255;

struct Signal {
    state: AtomicU8,
    owning_thread: Thread,
}

impl Signal {
    #[inline(always)]
    fn new() -> Self {
        Self {
            state: AtomicU8::new(WAITING),
            owning_thread: std::thread::current(),
        }
    }

    fn wait(&self) {
        // Try to fetch in short spin
        for _ in 0..16 {
            if self
                .state
                .compare_exchange(NOTIFIED, WAITING, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                return;
            }
            yield_now();
        }
        // Park current thread
        if self
            .state
            .compare_exchange(WAITING, PARKED, Ordering::AcqRel, Ordering::Relaxed)
            .is_err()
        {
            // already notified, reset state to waiting
            self.state.store(WAITING, Ordering::Release);
            return;
        }
        while self
            .state
            .compare_exchange(NOTIFIED, WAITING, Ordering::AcqRel, Ordering::Relaxed)
            .is_err()
        {
            std::thread::park();
        }
    }

    #[inline(always)]
    fn notify(&self) {
        if self.state.swap(NOTIFIED, Ordering::AcqRel) == PARKED {
            self.owning_thread.unpark();
        }
    }
}

impl Wake for Signal {
    #[inline(always)]
    fn wake(self: Arc<Self>) {
        self.notify();
    }
    #[inline(always)]
    fn wake_by_ref(self: &Arc<Self>) {
        self.notify();
    }
}

/// Blocks the current thread until the given future is ready.
///
/// # Example
///
/// ```
/// let my_fut = async {};
/// let result = swait::swait(my_fut);
/// ```
#[inline(always)]
pub fn swait<F: Future>(mut fut: F) -> F::Output {
    let mut fut = pin!(fut);
    THREAD_SIGNAL.with(|signal| {
        let waker = Waker::from(Arc::clone(signal));
        let mut context = Context::from_waker(&waker);

        loop {
            match fut.as_mut().poll(&mut context) {
                Poll::Pending => signal.wait(),
                Poll::Ready(result) => return result,
            }
        }
    })
}
