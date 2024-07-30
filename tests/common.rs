use std::thread;
use std::time::{Duration, Instant};
use swait::*;

// Test to ensure the basic functionality of swait works as expected
#[test]
fn test_swait_basic() {
    let future = async { 42 };
    let result = future.swait();
    assert_eq!(result, 42);
}

// Test to ensure swait works with a future that takes some time to complete
#[test]
fn test_swait_delayed() {
    let future = async {
        thread::sleep(Duration::from_millis(50));
        42
    };
    let start = Instant::now();
    let result = future.swait();
    let duration = start.elapsed();
    assert_eq!(result, 42);
    assert!(duration >= Duration::from_millis(50));
}

// Test to ensure swait works with futures that are already ready
#[test]
fn test_swait_already_ready() {
    let future = async { 42 };
    let result = future.swait();
    assert_eq!(result, 42);
}

// Test to ensure that swait can handle futures that panic
#[test]
#[should_panic(expected = "Intentional panic for testing")]
fn test_swait_future_panic() {
    let future = async { panic!("Intentional panic for testing") };
    let _ = future.swait();
}
