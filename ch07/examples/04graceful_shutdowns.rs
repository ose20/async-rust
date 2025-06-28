use std::cell::UnsafeCell;
use std::collections::HashMap;
use std::sync::LazyLock;
use std::thread::sleep;
use std::time::Instant;
use tokio_util::task::LocalPoolHandle;

thread_local! {
    pub static COUNTER: UnsafeCell<HashMap<u32, u32>> = UnsafeCell::new(HashMap::new());
}

static RUNTIME: LazyLock<LocalPoolHandle> = LazyLock::new(|| LocalPoolHandle::new(4));

async fn something(number: u32) {
    tokio::time::sleep(std::time::Duration::from_secs(number as u64)).await;
    COUNTER.with(|counter| {
        let counter = unsafe { &mut *counter.get() };
        match counter.get_mut(&number) {
            Some(count) => {
                let placeholder = *count + 1;
                *count = placeholder;
            }
            None => {
                counter.insert(number, 1);
            }
        }
    })
}

async fn print_statement() {
    COUNTER.with(|counter| {
        let counter = unsafe { &mut *counter.get() };
        println!("Counter: {:?}", counter);
    })
}

async fn cleanup() {
    println!("cleanup background task started");
    let mut count = 0;
    loop {
        tokio::signal::ctrl_c().await.unwrap();
        println!("ctrl-c received!");
        count += 1;
        if count > 2 {
            std::process::exit(0);
        }
    }
}

fn extract_data_from_thread() -> HashMap<u32, u32> {
    let mut extracted_data = HashMap::new();
    COUNTER.with(|counter| {
        let counter = unsafe { &mut *counter.get() };
        extracted_data = counter.clone();
    });
    extracted_data
}

async fn get_complete_count() -> HashMap<u32, u32> {
    let mut complete_counter = HashMap::new();
    let mut extracted_counters = Vec::new();
    for i in 0..4 {
        extracted_counters
            .push(RUNTIME.spawn_pinned_by_idx(|| async move { extract_data_from_thread() }, i));
    }
    for counter_future in extracted_counters {
        let extracted_counter = counter_future.await.unwrap_or_default();
        for (key, count) in extracted_counter {
            *complete_counter.entry(key).or_insert(0) += count;
        }
    }
    complete_counter
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    std::thread::spawn(|| async {
        let sequence = [1, 2, 3, 4, 5];
        let repeated_sequence: Vec<_> = sequence.iter().cycle().take(50000).cloned().collect();
        let mut futures = Vec::new();
        for num in repeated_sequence {
            futures.push(RUNTIME.spawn_pinned(move || async move {
                something(num).await;
                something(num).await;
            }));
        }
        for i in futures {
            i.await.unwrap();
        }
        println!("All futures completed");
    });
    tokio::signal::ctrl_c().await.unwrap();
    println!("ctrl-c received!");
    let complete_counter = get_complete_count().await;
    println!("Complete Counter: {:?}", complete_counter);
}
