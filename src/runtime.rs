use tokio::sync::mpsc;

use crate::pomodoro::Pomodoro;

enum Signal {
    Abort,
    Pause,
    Resume,
}

pub async fn start(mut pomodoro: Pomodoro) -> Client {
    let (sender, mut receiver) = mpsc::channel::<Signal>(2);
    let sender = sender.clone();
    let shared = pomodoro.shared.clone();
    tokio::spawn(async move {
        loop {
            pomodoro.run().await;
        }
    });
    let t = tokio::spawn( async move {
        loop {
            if let Some(signal) = receiver.recv().await {
                match signal {
                    Signal::Pause => shared.lock().unwrap().pause(),
                    Signal::Resume => shared.lock().unwrap().resume(),
                    Signal::Abort => return,
                }
            }
        }
    });
    tokio::join!(t);
    Client { sender }
}

pub struct Client {
    sender: mpsc::Sender<Signal>,
}

impl Client {
    async fn send_signal(&self, signal: Signal) {
        self.sender.send(signal).await;
    }

    pub async fn abort(&self) {
        self.send_signal(Signal::Abort).await;
    }

    pub async fn pause(&self) {
        self.send_signal(Signal::Pause).await;
    }

    pub async fn resume(&self) {
        self.send_signal(Signal::Resume).await;
    }
}

// #[tokio::test(flavor = "multi_thread", worker_threads = 3)]
// async fn test_main_loop() {
//     use std::time::Duration;
//
//     use tokio::time::sleep;
//
//     use crate::pomodoro::Clock;
//
//     let working = Clock::new(Duration::from_micros(1), Duration::from_micros(1));
//     let short_break = Clock::new(Duration::from_micros(1), Duration::from_micros(1));
//     let long_break = Clock::new(Duration::from_micros(1), Duration::from_micros(1));
//
//     let pomodoro = Pomodoro::new(working, short_break, long_break, 3, true, None);
//     let client= start(pomodoro).await;
//     sleep(Duration::from_micros(7)).await;
//     client.pause().await;
//     sleep(Duration::from_micros(7)).await;
//     client.resume().await;
//     sleep(Duration::from_micros(7)).await;
//     client.pause().await;
// }