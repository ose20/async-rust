use std::{
    sync::{Arc, Mutex},
    task::{Poll, Waker},
};

use tokio::{sync::mpsc, task};

struct MyFuture {
    state: Arc<Mutex<MyFutureState>>,
}

struct MyFutureState {
    data: Option<Vec<u8>>,
    waker: Option<Waker>,
}

impl MyFuture {
    fn new() -> (Self, Arc<Mutex<MyFutureState>>) {
        let state = Arc::new(Mutex::new(MyFutureState {
            data: None,
            waker: None,
        }));
        (
            MyFuture {
                state: state.clone(),
            },
            state,
        )
    }
}

impl Future for MyFuture {
    type Output = String;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        println!("Polling the future");
        let mut state = self.state.lock().unwrap();

        if state.data.is_some() {
            let data = state.data.take().unwrap();
            Poll::Ready(String::from_utf8(data).unwrap())
        } else {
            state.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

#[tokio::main]
async fn main() {
    let (my_future, state) = MyFuture::new();
    let (tx, mut rx) = mpsc::channel::<()>(1);
    let task_handle = task::spawn(async { my_future.await });
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    let trigger_task = task::spawn(async move {
        println!("spawning trigger task");
        rx.recv().await;
        let mut state = state.lock().unwrap();
        state.data = Some(b"Hello from the outsidee".to_vec());
        loop {
            if let Some(waker) = state.waker.take() {
                waker.wake();
                break;
            }
        }
    });
    tx.send(()).await.unwrap();

    let outcome = task_handle.await.unwrap();
    println!("Task completed with outcome: {}", outcome);
    trigger_task.await.unwrap();
}
