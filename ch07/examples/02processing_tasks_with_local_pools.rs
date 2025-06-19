use std::cell::RefCell;
use std::time::Duration;
use tokio_util::task::LocalPoolHandle;

// スレッドごとに独立したカウンターを持つために、thread_local!を使用する
// つまりスレッドが違えばカウンターも違う
thread_local! {
    pub static COUNTER: RefCell<u32> = RefCell::new(1);
}

async fn something(number: u32) -> u32 {
    std::thread::sleep(Duration::from_secs(3));
    COUNTER.with(|counter| {
        let mut count = counter.borrow_mut();
        *count += 1;
        *count
    })
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let pool = LocalPoolHandle::new(1);

    let one = pool.spawn_pinned(|| async {
        println!("one");
        something(1).await
    });
    let two = pool.spawn_pinned(|| async {
        println!("two");
        something(2).await
    });
    let three = pool.spawn_pinned(|| async {
        println!("three");
        something(3).await
    });

    let result = async {
        let one = one.await.unwrap();
        let two = two.await.unwrap();
        let three = three.await.unwrap();
        one + two + three
    };
    println!("result: {}", result.await);
}
