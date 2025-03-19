# `swait` - A Simple Utility for Blocking synchronously on Futures

[![Crates.io][crates-badge]][crates-url]
[![Documentation][doc-badge]][doc-url]
[![MIT licensed][mit-badge]][mit-url]

[crates-badge]: https://img.shields.io/crates/v/swait.svg?style=for-the-badge
[crates-url]: https://crates.io/crates/swait
[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg?style=for-the-badge
[mit-url]: https://github.com/fereidani/swait/blob/master/LICENSE
[doc-badge]: https://img.shields.io/docsrs/swait?style=for-the-badge
[doc-url]: https://docs.rs/swait

The `swait` library provides a utility to block the current thread until a given future is ready. This is particularly useful in scenarios where asynchronous operations need to be synchronized with blocking code.
The name `swait` is derived from the term `await` in Rust, indicating a synchronous wait operation.
The `swait` library originated from an attempt to improve the performance of the `pollster` crate through a pull request, which ultimately did not get merged.

## Features

- **Blocking on Futures:** The primary feature of this library is the ability to block the current thread until a future resolves, providing a seamless bridge between asynchronous and synchronous code.
- **Highly Optimized:** It uses a lock-free algorithm that provides a highly optimized way to synchronously wait for a future.
- **No Unsafe** `swait` is simple and does not use any unsafe code.

## Installation

To include `swait` in your project, add the following to your `Cargo.toml`:

```toml
[dependencies]
swait = "0.1"
```

Then, include it in your project:

```rust
use swait::FutureExt;
```

## Usage

The main API provided by `swait` is the `FutureExt` trait, which extends the functionality of Rust's `Future` trait with the `swait` method. This method blocks the current thread until the future is resolved.

### Example

```rust
use swait::FutureExt;

async fn async_operation() -> i32 {
    // Simulating an asynchronous operation
    42
}

fn main() {
    let result = async_operation().swait();
    println!("The result is: {}", result);
}
```

In this example, `async_operation()` is an asynchronous function that returns an `i32`. By calling `swait()` on it, the main thread blocks until the result is available, and then it prints the result.

## API Documentation

### `FutureExt` Trait

#### `swait`

```rust,ignore
fn swait(self) -> Self::Output
where
    Self: Sized;
```

This method blocks the current thread until the future is ready and returns the output of the future. It is implemented for all types that implement the `Future` trait.

### `swait` Function

```rust,ignore
pub fn swait<F: Future>(fut: F) -> F::Output
```

The `swait` function is a standalone function that takes a future as an argument and blocks the current thread until the future is resolved.

## Internal Implementation

The core of `swait` is built around a `Signal` structure, which manages the state of the waiting process. The `Signal` structure uses atomic operations and thread parking/unparking to efficiently wait for the future to complete.

### `Signal` Structure

The `Signal` structure manages the state of a waiting thread and provides methods to wait for and notify the thread. It uses an `AtomicU8` to track the state, which can be `WAITING`, `PARKED`, or `NOTIFIED`.

- **`wait` Method:** This method blocks the thread using a combination of spinning, yielding, and parking.
- **`notify` Method:** This method notifies and wakes the thread if it is parked.

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.

## Contribution

Contributions are welcome! Please submit a pull request or open an issue to discuss your ideas.
