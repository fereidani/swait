#![doc = include_str!("../README.md")]

use branches::{likely, unlikely};
use std::{
    future::Future,
    hint::spin_loop,
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

struct Signal {
    owning_thread: Thread,
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
        }
    }

    fn wait<F: Future>(&self, context: &mut Context<'_>, mut fut: Pin<&mut F>) -> F::Output {
        if is_multithreaded_env() {
            // exit early if predicate is already satisfied
            if let Poll::Ready(result) = fut.as_mut().poll(context) {
                return result;
            }
            const SPINING_COUNT: usize = 5;
            const YIELD_COUNT: usize = 5;
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

#[inline(always)]
fn is_multithreaded_env() -> bool {
    static PARALLELISM: AtomicUsize = AtomicUsize::new(0);
    let parallelism = PARALLELISM.load(Ordering::Relaxed);
    if unlikely(parallelism == 0) {
        let parallelism: usize =
            usize::from(available_parallelism().unwrap_or(std::num::NonZero::new(1).unwrap()));
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
    THREAD_SIGNAL.with(|signal| {
        let waker = Waker::from(Arc::clone(signal));
        let mut context = Context::from_waker(&waker);
        match fut.as_mut().poll(&mut context) {
            Poll::Pending => signal.wait(&mut context, fut.as_mut()),
            Poll::Ready(result) => result,
        }
    })
}
