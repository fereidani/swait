#![doc = include_str!("../README.md")]

use branches::{likely, unlikely};
use std::{
    future::Future,
    hint::spin_loop,
    marker::PhantomPinned,
    mem::forget,
    pin::*,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    task::*,
    thread::{available_parallelism, yield_now as os_yield, Thread},
};

thread_local! {
    // A reusable signal instance per thread.
    static THREAD_SIGNAL: (Arc<Signal>, Waker) = {
        // Pinned boxed signal to ensure it has a stable address and upholds Pin guarantees
        let signal = Arc::new(Signal::new());
        let waker = signal.get_waker();
        (signal, waker)
    };
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

struct Signal {
    owning_thread: Thread,
    _pin: PhantomPinned,
}

macro_rules! return_if_ready {
    ($fut:expr,$context:expr) => {
        let poll_result = $fut.as_mut().poll($context);
        if likely(poll_result.is_ready()) {
            if let Poll::Ready(result) = poll_result {
                return result;
            }
        }
    };
}

impl Signal {
    #[inline(always)]
    fn new() -> Self {
        Self {
            owning_thread: std::thread::current(),
            _pin: PhantomPinned,
        }
    }

    /// Creates a waker that notifies this signal when woken.
    fn get_waker(self: &Arc<Signal>) -> Waker {
        static VTABLE: RawWakerVTable = RawWakerVTable::new(
            |data: *const ()| {
                // SAFETY: we have owning reference here and it is safe to clone it
                let this = unsafe { Arc::from_raw(data as *const Signal) };
                let clone = Arc::into_raw(this.clone());
                // avoid decreasing the ref count after cloning
                forget(this);
                RawWaker::new(clone as *const (), &VTABLE)
            },
            |data: *const ()| unsafe {
                // SAFETY: we have owning reference here and it is safe to use and destroy it
                (&*(data as *const Signal)).notify();
                let _ = Arc::from_raw(data as *const Signal);
            },
            |data: *const ()| unsafe {
                // SAFETY: we have owning reference here and it is safe to use it
                (&*(data as *const Signal)).notify();
            },
            |data: *const ()| unsafe {
                // SAFETY: we have owning reference here and we are safe to drop it
                let _ = Arc::from_raw(data as *const Signal);
            },
        );
        let sig_clone = Arc::into_raw(self.clone());
        let raw_waker = RawWaker::new(sig_clone as *const _ as *const (), &VTABLE);
        unsafe { Waker::from_raw(raw_waker) }
    }

    fn wait<F: Future>(&self, context: &mut Context<'_>, mut fut: Pin<&mut F>) -> F::Output {
        // exit early if predicate is already satisfied
        if let Poll::Ready(result) = fut.as_mut().poll(context) {
            return result;
        }
        const SPINING_COUNT: u32 = 5;
        const YIELD_COUNT: u32 = 5;
        // skip busy-wait spinning if the environment is not multithreaded
        if is_multithreaded_env() {
            for shift in 1..(1 + SPINING_COUNT) {
                for _ in 0..1 << shift {
                    spin_loop();
                }
                return_if_ready!(fut, context);
            }
            for _ in 0..YIELD_COUNT {
                os_yield();
                return_if_ready!(fut, context);
            }
        } else {
            // in single threaded environment busy-spinning just wastes CPU cycles
            // we only use os yield syscall to deschedule the thread
            for _ in 0..(YIELD_COUNT + SPINING_COUNT) {
                os_yield();
                return_if_ready!(fut, context);
            }
        }

        // park the thread early so we don't poll again
        std::thread::park();
        loop {
            match fut.as_mut().poll(context) {
                Poll::Ready(result) => return result,
                Poll::Pending => {
                    // if it is still pending park the thread
                    std::thread::park();
                }
            }
        }
    }

    #[inline(always)]
    fn notify(&self) {
        self.owning_thread.unpark();
    }
}

#[inline(always)]
fn is_multithreaded_env() -> bool {
    static PARALLELISM: AtomicUsize = AtomicUsize::new(0);
    let parallelism = PARALLELISM.load(Ordering::Relaxed);
    if unlikely(parallelism == 0) {
        let parallelism: usize = available_parallelism().map(|n| n.get()).unwrap_or(1);
        PARALLELISM.store(parallelism, Ordering::Relaxed);
        parallelism > 1
    } else {
        parallelism > 1
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
///
/// # Example 2
///
/// ```
/// use swait::FutureExt;
/// let my_fut = async {};
/// let result = my_fut.swait();
/// ```
#[inline(always)]
pub fn swait<F: Future>(fut: F) -> F::Output {
    let mut fut = pin!(fut);
    THREAD_SIGNAL.with(|(signal, waker)| {
        let mut context = Context::from_waker(waker);
        match fut.as_mut().poll(&mut context) {
            Poll::Pending => signal.wait(&mut context, fut.as_mut()),
            Poll::Ready(result) => result,
        }
    })
}
