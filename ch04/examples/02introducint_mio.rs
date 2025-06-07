use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interest, Poll as MioPoll, Token};
use std::error::Error;
use std::io::{Read, Write};
use std::time::Duration;

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use std::sync::LazyLock;
use std::{panic::catch_unwind, thread};

use async_task::{Runnable, Task};
use flume::{Receiver, Sender};
use futures_lite::future;

const SERVER: Token = Token(0);
const CLIENT: Token = Token(1);

struct ServerFuture {
    server: TcpListener,
    // to pool the socket and tell the future when the socket is readable.
    poll: MioPoll,
}

impl Future for ServerFuture {
    type Output = String;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut events = Events::with_capacity(1);

        // 200ms 間で event に Event をため込む
        self.poll
            .poll(&mut events, Some(Duration::from_millis(200)))
            .unwrap();

        for event in events.iter() {
            // このイベントには token が SERVER じゃないものも来ることを示唆している
            // このイベントって、OS 全体のやつなのかな？それともこの Rust プログラムに関するものだけ？
            // Grok3曰く、この Rust プログラムに関するものだけらしい
            if event.token() == SERVER && event.is_readable() {
                let (mut stream, _) = self.server.accept().unwrap();
                let mut buffer = [0u8; 1024];
                let mut received_data = Vec::new();

                loop {
                    match stream.read(&mut buffer) {
                        Ok(n) if n > 0 => {
                            received_data.extend_from_slice(&buffer[..n]);
                        }
                        Ok(_) => {
                            break;
                        }
                        // これ本に書いてあるけど実行すると何も出力されなくなる
                        // cx..., return..., じゃなくて break にすると正常に動く(実質的に次の節と同じなのでそれはそう)
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            // WouldBlock エラーは、読み取り可能なデータがないことを示す
                            println!("No more data to read, waiting for more...");
                            break;
                        }
                        Err(e) => {
                            eprintln!("Error reading from stream: {}", e);
                            break;
                        }
                    }
                }

                if !received_data.is_empty() {
                    let received_str = String::from_utf8_lossy(&received_data);
                    return Poll::Ready(received_str.to_string());

                    // もしプログラム終了まで socket が poll され続けてほしいなら以下のようにする
                    // spawn_task!(some_async_handle_function(&received_data)).detach();
                    // return Poll::Pending;
                }

                // ここに辿り着いてしまうと hang するように見える
                // この実装は self に状態を保存していないから
                // poll はステートマシンを一歩進めるみたいな直観をもっていたいっぽい（だから状態の更新をしたい）
                // 本の例でも実はここには辿り着いてない
                // ↓ ChatGPT の説明
                // ただし ハングの直接原因 は TcpStream と受信バッファを self に保持していないこと。
                // 末尾に到達したこと自体が悪いのではなく、次回 poll() が呼ばれたときに 続きのステートが無い ので、
                // 毎回ほぼ同じ処理（accept()→WouldBlock）を繰り返し、永久に Poll::Pending を返し続ける状態に陥る。
                cx.waker().wake_by_ref();
                println!("poll function ended, waiting for more data...");
                return Poll::Pending;
            }
        }

        cx.waker().wake_by_ref();
        Poll::Pending
    }
}

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

#[allow(unused_macros)]
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

#[derive(Debug, Clone, Copy)]
struct BackgroundProcess;

impl Future for BackgroundProcess {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        println!("Running background process...");
        std::thread::sleep(Duration::from_secs(1));
        cx.waker().wake_by_ref();
        Poll::Pending
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    Runtime::new().with_low_num(2).with_high_num(4).run();

    let addr = "127.0.0.1:13265".parse()?;
    let mut server = TcpListener::bind(addr)?;
    let mut stream = TcpStream::connect(server.local_addr()?)?;

    let poll: MioPoll = MioPoll::new()?;
    poll.registry()
        .register(&mut server, SERVER, Interest::READABLE)?;

    let server_worker = ServerFuture { server, poll };
    let test = spawn_task!(server_worker);

    let mut client_poll: MioPoll = MioPoll::new()?;
    client_poll
        .registry()
        .register(&mut stream, CLIENT, Interest::WRITABLE)?;

    let mut events = Events::with_capacity(128);
    client_poll.poll(&mut events, None).unwrap();

    for event in events.iter() {
        if event.token() == CLIENT && event.is_writable() {
            let message = "that's so dingo!\n";
            let _ = stream.write_all(message.as_bytes());
        }
    }

    let outcome = future::block_on(test);
    println!("outcome: {outcome}");

    Ok(())
}
