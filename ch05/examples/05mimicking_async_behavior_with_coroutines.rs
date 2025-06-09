#![feature(coroutines)]
#![feature(coroutine_trait)]

use std::{
    collections::VecDeque,
    future::Future,
    ops::{Coroutine, CoroutineState},
    pin::Pin,
    task::{Context, Poll},
    time::Instant,
};

struct Excecutor {
    coroutines: VecDeque<Pin<Box<dyn Coroutine<(), Yield = (), Return = ()>>>>,
}

impl Excecutor {
    fn new() -> Self {
        Self {
            coroutines: VecDeque::new(),
        }
    }

    fn add(&mut self, coroutine: Pin<Box<dyn Coroutine<(), Yield = (), Return = ()>>>) {
        self.coroutines.push_back(coroutine);
    }

    // Queue からコルーチンを取り出して実行する
    // 完了しなかったら再度 Queue に戻す
    fn poll(&mut self) {
        println!("Polling {} coroutines", self.coroutines.len());
        let mut coroutine = self.coroutines.pop_front().unwrap();
        if let CoroutineState::Yielded(_) = coroutine.as_mut().resume(()) {
            self.coroutines.push_back(coroutine);
        }
    }
}

// pausing of execution のデモ
struct SleepCoroutine {
    pub start: Instant,
    pub duration: std::time::Duration,
}

impl SleepCoroutine {
    fn new(duration: std::time::Duration) -> Self {
        Self {
            start: Instant::now(),
            duration,
        }
    }
}

impl Coroutine<()> for SleepCoroutine {
    type Yield = ();
    type Return = ();

    // このメソッドを呼び出した構造体の作成時からの経過時間が
    // その構造体に設定された duration を超えている場合のみ、またそのときのみ完了する
    fn resume(self: Pin<&mut Self>, _arg: ()) -> CoroutineState<Self::Yield, Self::Return> {
        if self.start.elapsed() >= self.duration {
            CoroutineState::Complete(())
        } else {
            CoroutineState::Yielded(())
        }
    }
}

impl Future for SleepCoroutine {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match Pin::new(&mut self).resume(()) {
            CoroutineState::Complete(_) => Poll::Ready(()),
            CoroutineState::Yielded(_) => {
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }
}

fn main() {
    let mut executor = Excecutor::new();

    for _ in 0..3 {
        let coroutine = SleepCoroutine::new(std::time::Duration::from_secs(1));
        executor.add(Box::pin(coroutine));
    }

    let start = Instant::now();
    while !executor.coroutines.is_empty() {
        executor.poll();
    }

    println!("Took {:?}", start.elapsed());
}
