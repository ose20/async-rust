use std::pin::Pin;
use std::sync::LazyLock;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use std::{future::Future, panic::catch_unwind, thread};

use async_task::{Runnable, Task};
use futures_lite::future;

use http::Uri;
use hyper::{Client, Request};

fn spawn_task<F, T>(future: F) -> Task<T>
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    // static lifetime にしている
    // プログラム全体にわたってタスクを queue に入れたりするから
    // LazyLock は最初に呼ばれた時だけ初期化する
    // おそらく、いろんな spawn_task が呼ばれると思うけど、そのすべてでこれを共有するのだろう
    static QUEUE: LazyLock<flume::Sender<Runnable>> = LazyLock::new(|| {
        let (tx, rx) = flume::unbounded::<Runnable>();

        thread::spawn(move || {
            while let Ok(runnable) = rx.recv() {
                println!("runnable accepted");
                let _ = catch_unwind(|| runnable.run());
            }
        });
        tx
    });

    // まだ async_task crate を使う
    // std だけつかって自作するのは ch10
    let schedule = |runnable| QUEUE.send(runnable).unwrap();
    let (runnable, task) = async_task::spawn(future, schedule);
    runnable.schedule();
    println!("Here is the queue count: {:?}", QUEUE.len());
    task
}

struct AsyncSleep {
    start_time: Instant,
    duration: Duration,
}
impl AsyncSleep {
    #[allow(dead_code)]
    fn new(duration: Duration) -> Self {
        Self {
            start_time: Instant::now(),
            duration,
        }
    }
}

impl Future for AsyncSleep {
    type Output = bool;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let elapsed_time = self.start_time.elapsed();
        if elapsed_time >= self.duration {
            Poll::Ready(true)
        } else {
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

fn main() {
    let url = "https://www.rust-lang.org";
    let uri: Uri = url.parse().expect("Failed to parse URI");

    let request = Request::builder()
        .method("GET")
        .uri(uri)
        .header("User-Agent", "hyper/0.14.2")
        .body(hyper::Body::empty())
        .expect("Failed to build request");

    let future = async {
        let client = Client::new();
        client.request(request).await.unwrap()
    };

    // 実行すると以下のようなランタイムエラーが出る

    // thread '<unnamed>' panicked at examples/01integrating_hyper_into_our_async_runtime.rs:84:39:
    // called `Result::unwrap()` on an `Err` value: hyper::Error(Connect, "invalid URL, scheme is not http")

    let test = spawn_task(future);
    let response = future::block_on(test);
    println!("Response: {:?}", response);
}
