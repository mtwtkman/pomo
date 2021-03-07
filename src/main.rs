use std::time::Duration;

mod pomodoro;
mod runtime;

use pomodoro::{Clock, Pomodoro};
use runtime::start;

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let working = Clock::new(Duration::from_secs(5), Duration::from_secs(1));
    let short_break = Clock::new(Duration::from_secs(3), Duration::from_secs(1));
    let long_break = Clock::new(Duration::from_secs(4), Duration::from_secs(1));
    let pomo = Pomodoro::new(working, short_break, long_break, 2, true, None);
    let client = start(pomo).await;
}
