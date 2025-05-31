use std::{
    sync::{Arc, Condvar, Mutex, atomic::AtomicBool},
    thread,
    time::Duration,
};

fn main() {
    let shared_data = Arc::new((Mutex::new(false), Condvar::new()));
    let shared_data_clone = Arc::clone(&shared_data);
    let STOP = Arc::new(AtomicBool::new(false));
    let STOP_CLONE = Arc::clone(&STOP);

    let background_thread = thread::spawn(move || {
        let (lock, cvar) = &*shared_data_clone;
        let mut received_value = lock.lock().unwrap();
        while !STOP.load(std::sync::atomic::Ordering::Relaxed) {
            received_value = cvar.wait(received_value).unwrap();
            println!("Received value: {}", received_value);
        }
    });

    let updater_thread = thread::spawn(move || {
        let (lock, cvar) = &*shared_data;
        let values = [false, true, false, true];

        for i in 0..4 {
            let update_value = values[i as usize];
            println!("Updatging value to {}...", update_value);
            *lock.lock().unwrap() = update_value;
            cvar.notify_one();
            thread::sleep(Duration::from_secs(4));
        }
        STOP_CLONE.store(true, std::sync::atomic::Ordering::Relaxed);
        println!("STOP has been updated");
        cvar.notify_one();
    });
    updater_thread.join().unwrap();
}
