use std::pin::Pin;
use std::sync::LazyLock;
use std::task::{Context, Poll};
use std::time::Duration;
use std::{future::Future, panic::catch_unwind, thread};

use async_task::{Runnable, Task};
use futures_lite::future;

#[derive(Debug, Clone, Copy)]
enum FutureType {
    High,
    Low,
}

trait FutureOrderLabel: Future {
    fn get_order(&self) -> FutureType;
}

fn spawn_task<F, T>(future: F) -> Task<T>
where
    F: Future<Output = T> + Send + 'static + FutureOrderLabel,
    T: Send + 'static,
{
    // static lifetime にしている
    // プログラム全体にわたってタスクを queue に入れたりするから
    // LazyLock は最初に呼ばれた時だけ初期化する
    // おそらく、いろんな spawn_task が呼ばれると思うけど、そのすべてでこれを共有するのだろう
    static HIGH_QUEUE: LazyLock<flume::Sender<Runnable>> = LazyLock::new(|| {
        let (tx, rx) = flume::unbounded::<Runnable>();

        for _ in 0..2 {
            let receiver = rx.clone();
            thread::spawn(move || {
                while let Ok(runnable) = receiver.recv() {
                    let _ = catch_unwind(|| runnable.run());
                }
            });
        }
        tx
    });

    static LOW_QUEUE: LazyLock<flume::Sender<Runnable>> = LazyLock::new(|| {
        let (tx, rx) = flume::unbounded::<Runnable>();
        for _ in 0..1 {
            let receiver = rx.clone();
            thread::spawn(move || {
                while let Ok(runnable) = receiver.recv() {
                    let _ = catch_unwind(|| runnable.run());
                }
            });
        }

        tx
    });

    // まだ async_task crate を使う
    // std だけつかって自作するのは ch10
    let schedule_high = |runnable| HIGH_QUEUE.send(runnable).unwrap();
    let schedule_low = |runnable| LOW_QUEUE.send(runnable).unwrap();

    let schedule = match future.get_order() {
        FutureType::High => schedule_high,
        FutureType::Low => schedule_low,
    };
    let (runnable, task) = async_task::spawn(future, schedule);
    runnable.schedule();
    task
}

struct CounterFuture {
    count: u32,
    order: FutureType,
}

impl FutureOrderLabel for CounterFuture {
    fn get_order(&self) -> FutureType {
        self.order
    }
}

impl Future for CounterFuture {
    type Output = u32;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.count += 1;
        println!("polling with result: {}", self.count);
        std::thread::sleep(Duration::from_secs(1));
        if self.count < 3 {
            cx.waker().wake_by_ref();
            Poll::Pending
        } else {
            Poll::Ready(self.count)
        }
    }
}

fn main() {
    let one = CounterFuture {
        count: 0,
        order: FutureType::High,
    };
    let two = CounterFuture {
        count: 0,
        order: FutureType::Low,
    };
    let t_one = spawn_task(one);
    let t_two = spawn_task(two);
    std::thread::sleep(Duration::from_secs(5));
    println!("before the block");
    future::block_on(t_one);
    future::block_on(t_two);
}
