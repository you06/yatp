// Copyright 2021 TiKV Project Authors. Licensed under Apache-2.0.

use criterion::*;
use std::time::{Instant, Duration};
use std::task::{Context, Poll};
use std::future::*;
use std::pin::Pin;

struct WaitUntil(Instant);

impl WaitUntil {
    fn new(duration: Duration) -> Self {
        WaitUntil(Instant::now().checked_add(duration).unwrap())
    }

    fn cb_poll(&self) -> bool {
        let now = Instant::now();
        if self.0 <= now {
            true
        } else {
            false
        }
    }
}

impl Future for WaitUntil {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let now = Instant::now();
        if self.0 <= now {
            Poll::Ready(())
        } else {
            cx.waker().clone().wake();
            Poll::Pending
        }
    }
}

mod yatp_callback {
    use criterion::*;
    use std::sync::*;
    use std::time::Duration;
    use yatp::queue::*;
    use yatp::task::callback::{Task, TaskCell};

    pub fn wait_switch(b: &mut Bencher<'_>, wait_count: usize) {
        let pool = yatp::Builder::new("wait_switch").build_callback_pool();
        let (tx, rx) = mpsc::sync_channel(1000);
        let tasks = wait_count * num_cpus::get();

        b.iter(|| {
            for _ in 0..tasks {
                let tx = tx.clone();
                let wait_until = super::WaitUntil::new(Duration::from_millis(10));

                pool.spawn(TaskCell {
                    task: Task::new_mut(move |h: &mut yatp::task::callback::Handle<'_>| {
                        if wait_until.cb_poll() {
                            tx.send(()).unwrap();
                        } else {
                            h.set_rerun(true);
                        }
                    }),
                    extras: Extras::single_level(),
                });
            }

            for _ in 0..tasks {
                let _ = rx.recv().unwrap();
            }
        });
    }
}

mod yatp_future {
    use criterion::*;
    use yatp::ThreadPool;
    use yatp::task::future::TaskCell;
    use std::sync::*;
    use std::time::Duration;

    fn wait_switch(pool: ThreadPool<TaskCell>, b: &mut Bencher<'_>, wait_count: usize) {
        let (tx, rx) = mpsc::sync_channel(1000);
        let tasks = wait_count * num_cpus::get();

        b.iter(|| {
            for _ in 0..tasks {
                let tx = tx.clone();

                pool.spawn(async move {
                    let wait_until = super::WaitUntil::new(Duration::from_millis(10));
                    wait_until.await;
                    tx.send(()).unwrap();
                });
            }

            for _ in 0..tasks {
                let _ = rx.recv().unwrap();
            }
        });
    }

    pub fn wait_switch_single_level(b: &mut Bencher<'_>, wait_count: usize) {
        let pool = yatp::Builder::new("wait_switch").build_future_pool();
        wait_switch(pool, b, wait_count);
    }

    pub fn wait_switch_multi_level(b: &mut Bencher<'_>, wait_count: usize) {
        let pool = yatp::Builder::new("wait_switch").build_multilevel_future_pool();
        wait_switch(pool, b, wait_count);
    }
}

mod tokio {
    use criterion::*;
    use tokio::runtime::Builder;
    use std::sync::*;
    use std::time::Duration;

    pub fn wait_switch(b: &mut Bencher<'_>, wait_count: usize) {
        let pool = Builder::new_multi_thread()
            .worker_threads(num_cpus::get())
            .build()
            .unwrap();
        let (tx, rx) = mpsc::sync_channel(1000);
        let tasks = wait_count * num_cpus::get();

        b.iter(|| {
            for _ in 0..tasks {
                let tx = tx.clone();

                pool.spawn(async move {
                    let wait_until = super::WaitUntil::new(Duration::from_millis(10));
                    wait_until.await;
                    tx.send(()).unwrap();
                });
            }

            for _ in 0..tasks {
                let _ = rx.recv().unwrap();
            }
        });
    }
}

mod async_std {
    use criterion::*;
    use std::sync::*;
    use std::time::Duration;

    pub fn wait_switch(b: &mut Bencher<'_>, wait_count: usize) {
        
        let (tx, rx) = mpsc::sync_channel(1000);
        let tasks = wait_count * num_cpus::get();

        b.iter(|| {
            for _ in 0..tasks {
                let tx = tx.clone();

                async_std::task::spawn(async move {
                    let wait_until = super::WaitUntil::new(Duration::from_millis(10));
                    wait_until.await;
                    tx.send(()).unwrap();
                });
            }

            for _ in 0..tasks {
                let _ = rx.recv().unwrap();
            }
        });
    }
}

pub fn wait_switch(b: &mut Criterion) {
    let mut group = b.benchmark_group("wait_switch");
    for i in &[8, 16, 32, 64] {
        group.bench_with_input(BenchmarkId::new("yatp::callback", i), i, |b, i| {
            yatp_callback::wait_switch(b, *i)
        });
        group.bench_with_input(BenchmarkId::new("yatp::future::single_level", i), i, |b, i| {
            yatp_future::wait_switch_single_level(b, *i)
        });
        group.bench_with_input(BenchmarkId::new("yatp::future::multi_level", i), i, |b, i| {
            yatp_future::wait_switch_multi_level(b, *i)
        });
        group.bench_with_input(BenchmarkId::new("tokio", i), i, |b, i| {
            tokio::wait_switch(b, *i)
        });
        group.bench_with_input(BenchmarkId::new("async_std", i), i, |b, i| {
            async_std::wait_switch(b, *i)
        });
    }
    group.finish();
}

criterion_group!(wait_switch_group, wait_switch);

criterion_main!(wait_switch_group);
