use std::pin::Pin;
use std::sync::LazyLock;
use std::task::{Context, Poll};
use std::time::Duration;
use std::{future::Future, panic::catch_unwind, thread};

use async_task::{Runnable, Task};
use flume::{Receiver, Sender};
use futures_lite::future;

#[derive(Debug, Clone, Copy)]
enum FutureType {
    High,
    Low,
}

macro_rules! spawn_task {
    ($future:expr) => {
        spawn_task($future, FutureType::Low)
    };
    ($future:expr, $order:expr) => {
        spawn_task($future, $order)
    };
}

macro_rules! join {
    ($($future:expr),*) => {
        {
            let mut results = Vec::new();
            $(
                let result = future::block_on($future);
                results.push(result);
            )*
            results
        }
    };
}

macro_rules! try_join {
    ($($future:expr),*) => {
        {
            let mut results = Vec::new();
            $(
                let result = catch_unwind(|| future::block_on($future));
                results.push(result);
            )*
            results
        }
    };
}

fn spawn_task<F, T>(future: F, order: FutureType) -> Task<T>
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    static HIGH_CHANNEL: LazyLock<(Sender<Runnable>, Receiver<Runnable>)> =
        LazyLock::new(flume::unbounded::<Runnable>);
    static LOW_CHANNEL: LazyLock<(Sender<Runnable>, Receiver<Runnable>)> =
        LazyLock::new(flume::unbounded::<Runnable>);

    // static lifetime にしている
    // プログラム全体にわたってタスクを queue に入れたりするから
    // LazyLock は最初に呼ばれた時だけ初期化する
    // おそらく、いろんな spawn_task が呼ばれると思うけど、そのすべてでこれを共有するのだろう
    // これらは Lazy なので、はじめて呼ばれたときに初期化される
    // つまり、起動しないとその QUEUE が steal することもない
    static HIGH_QUEUE: LazyLock<flume::Sender<Runnable>> = LazyLock::new(|| {
        for _ in 0..2 {
            let high_receiver = HIGH_CHANNEL.1.clone();
            let low_receiver = LOW_CHANNEL.1.clone();
            thread::spawn(move || {
                loop {
                    match high_receiver.recv() {
                        Ok(runnable) => {
                            let _ = catch_unwind(|| runnable.run());
                        }
                        Err(_) => {
                            match low_receiver.recv() {
                                Ok(runnable) => {
                                    let _ = catch_unwind(|| runnable.run());
                                }
                                Err(_) => {
                                    // 受信できない場合はスリープしてから再試行
                                    thread::sleep(Duration::from_millis(100));
                                }
                            }
                        }
                    }
                }
            });
        }
        HIGH_CHANNEL.0.clone()
    });

    static LOW_QUEUE: LazyLock<flume::Sender<Runnable>> = LazyLock::new(|| {
        for _ in 0..1 {
            let low_receiver = LOW_CHANNEL.1.clone();
            thread::spawn(move || {
                loop {
                    match low_receiver.recv() {
                        Ok(runnable) => {
                            let _ = catch_unwind(|| runnable.run());
                        }
                        Err(_) => {
                            match HIGH_CHANNEL.1.recv() {
                                Ok(runnable) => {
                                    let _ = catch_unwind(|| runnable.run());
                                }
                                Err(_) => {
                                    // 受信できない場合はスリープしてから再試行
                                    thread::sleep(Duration::from_millis(100));
                                }
                            }
                        }
                    }
                }
            });
        }
        LOW_CHANNEL.0.clone()
    });

    // まだ async_task crate を使う
    // std だけつかって自作するのは ch10
    let schedule_high = |runnable| HIGH_QUEUE.send(runnable).unwrap();
    let schedule_low = |runnable| LOW_QUEUE.send(runnable).unwrap();

    let schedule = match order {
        FutureType::High => schedule_high,
        FutureType::Low => schedule_low,
    };
    let (runnable, task) = async_task::spawn(future, schedule);
    runnable.schedule();
    task
}

struct Runtime {
    high_num: usize,
    low_num: usize,
}

impl Runtime {
    pub fn new() -> Self {
        let num_cores = std::thread::available_parallelism().unwrap().get();

        Self {
            high_num: num_cores - 2,
            low_num: 1,
        }
    }

    pub fn with_high_num(mut self, high_num: usize) -> Self {
        self.high_num = high_num;
        self
    }

    pub fn with_low_num(mut self, low_num: usize) -> Self {
        self.low_num = low_num;
        self
    }

    pub fn run(&self) {
        unsafe { std::env::set_var("HIGH_NUM", self.high_num.to_string()) }
        unsafe { std::env::set_var("LOW_NUM", self.low_num.to_string()) }

        let high = spawn_task!(async {}, FutureType::High);
        let low = spawn_task!(async {}, FutureType::Low);
        join!(high, low);
    }
}
struct CounterFuture {
    count: u32,
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

async fn async_fn() {
    std::thread::sleep(Duration::from_secs(1));
    println!("async fn")
}

fn main() {
    Runtime::new().with_low_num(2).with_high_num(4).run();

    let one = CounterFuture { count: 0 };
    let two = CounterFuture { count: 0 };
    let t_one = spawn_task!(one, FutureType::High);
    let t_two = spawn_task!(two);
    let t_three = spawn_task!(
        async {
            async_fn().await;
            async_fn().await;
        },
        FutureType::High
    );
    std::thread::sleep(Duration::from_secs(5));
    println!("before the block");
    let _: Vec<_> = try_join!(t_one, t_two);
    let _: Vec<_> = try_join!(t_three);
}
