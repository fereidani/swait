use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::thread;
use swait::FutureExt;

async fn simple_async() -> isize {
    777
}

// Benchmark for a simple future that immediately completes.
fn bench_swait_basic(c: &mut Criterion) {
    c.bench_function("swait_basic", |b| {
        b.iter(|| {
            let result = black_box(simple_async()).swait();
            assert_eq!(result, 777);
        })
    });
}

// Benchmark for a simple future that immediately completes.
fn bench_pollster_basic(c: &mut Criterion) {
    use pollster::FutureExt;
    c.bench_function("pollster_basic", |b| {
        b.iter(|| {
            let result = black_box(simple_async()).block_on();
            assert_eq!(result, 777);
        })
    });
}

// Benchmark for receiving a message from another thread using kanal.
fn bench_message_passing_kanal_swait(c: &mut Criterion) {
    c.bench_function("message_passing_kanal_swait", |b| {
        let (sender, receiver) = kanal::bounded_async(0);

        let handle = thread::spawn(move || loop {
            if sender.send(777).swait().is_err() {
                break;
            }
        });
        b.iter(|| {
            // Receiving the message
            let received = receiver.recv().swait().unwrap();
            assert_eq!(received, 777);
        });
        drop(receiver);
        _ = handle.join();
    });
}

// Benchmark for receiving a message from another thread using kanal.
fn bench_message_passing_kanal_sync_api(c: &mut Criterion) {
    c.bench_function("message_passing_kanal_sync_api", |b| {
        let (sender, receiver) = kanal::bounded(0);

        let handle = thread::spawn(move || loop {
            if sender.send(777).is_err() {
                break;
            }
        });
        b.iter(|| {
            // Receiving the message
            let received = receiver.recv().unwrap();
            assert_eq!(received, 777);
        });
        drop(receiver);
        _ = handle.join();
    });
}

criterion_group!(
    benches,
    bench_message_passing_kanal_swait,
    bench_message_passing_kanal_sync_api,
    bench_swait_basic,
    bench_pollster_basic,
);
criterion_main!(benches);
