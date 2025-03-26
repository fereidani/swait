#![doc = include_str!("../README.md")]

use std::{
    future::Future,
    hint::spin_loop,
    pin::*,
    sync::{
        atomic::{AtomicU8, AtomicUsize, Ordering},
        Arc,
    },
    task::*,
    thread::{available_parallelism, yield_now, Thread},
};

use branches::{likely, unlikely};

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
        if likely(cond_spin(|| {
            self.state
                .compare_exchange(NOTIFIED, WAITING, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
        })) {
            return;
        }
        // Park current thread
        if likely(
            self.state
                .compare_exchange(WAITING, PARKED, Ordering::AcqRel, Ordering::Relaxed)
                .is_err(),
        ) {
            // already notified, reset state to waiting
            self.state.store(WAITING, Ordering::Release);
            return;
        }
        while unlikely(
            self.state
                .compare_exchange(NOTIFIED, WAITING, Ordering::AcqRel, Ordering::Relaxed)
                .is_err(),
        ) {
            std::thread::park();
        }
    }

    #[inline(always)]
    fn notify(&self) {
        if likely(self.state.swap(NOTIFIED, Ordering::AcqRel) == PARKED) {
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

#[inline(always)]
fn is_multithreaded_env() -> bool {
    static PARRALLELISM: AtomicUsize = AtomicUsize::new(0);
    let parrallelism = PARRALLELISM.load(Ordering::Relaxed);
    if parrallelism == 0 {
        let parallelism: usize =
            usize::from(available_parallelism().unwrap_or(std::num::NonZero::new(1).unwrap()));
        PARRALLELISM.store(parallelism, Ordering::Relaxed);
        parallelism > 1
    } else {
        parrallelism > 1
    }
}

/**
 * Attempts to satisfy a given predicate by first executing a series of busy-wait spin loops,
 * and if unsuccessful, by yielding the current thread.
 *
 * function marks all branches as `likely`` to help the compiler optimize the code for exiting
 * although it is unlikely in reality, it helps performance.
 *
 * Returns:
 *   - `true` if the predicate returns `true` during any of the spin or yield phases.
 *   - `false` if the predicate remains unmet after the advised spinning and yielding, suggesting
 *     that further spinning is unlikely to be beneficial and parking the thread may be more appropriate.
 */
#[inline(always)]
fn cond_spin(predicate: impl Fn() -> bool) -> bool {
    // exit early if predicate is already satisfied
    if likely(predicate()) {
        return true;
    }
    const SPINING_COUNT: usize = 5;
    const YIELD_COUNT: usize = 5;
    // skip busy-wait spinning if the environment is not multithreaded
    if is_multithreaded_env() {
        for shift in 1..(1 + SPINING_COUNT) {
            for _ in 0..1 << shift {
                spin_loop();
            }
            if likely(predicate()) {
                return true;
            }
        }
        for _ in 0..YIELD_COUNT {
            yield_now();
            if likely(predicate()) {
                return true;
            }
        }
    } else {
        for _ in 0..(YIELD_COUNT + SPINING_COUNT) {
            yield_now();
            if likely(predicate()) {
                return true;
            }
        }
    }
    return false;
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
