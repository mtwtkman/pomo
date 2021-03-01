use std::cell::Cell;
use std::time::Duration;
use std::sync::{Arc, Mutex, mpsc};

use tokio::time::sleep;


#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    Working,
    ShortBreak,
    LongBreak,
}

#[derive(Debug, Clone)]
struct Counter {
    working: u8,
    short_break: u8,
    long_break: u8,
}

impl Counter {
    fn new() -> Self {
        Self {
            working: 0,
            short_break: 0,
            long_break: 0,
        }
    }

    fn increment_working(&mut self) {
        self.working += 1;
    }

    fn increment_short_break(&mut self) {
        self.short_break += 1;
    }

    fn increment_long_break(&mut self) {
        self.long_break += 1;
    }
}

struct Shared {
    paused: Arc<Mutex<bool>>,
}

impl Shared {
    fn new() -> Self {
        Self { paused: Arc::new(Mutex::new(true)) }
    }

    fn pause(&mut self) {
        *self.paused.lock().unwrap() = true
    }

    fn resume(&self) {
        *self.paused.lock().unwrap() = false
    }
}

#[derive(Debug, Clone)]
pub struct Clock {
    lifespan: Duration,
    tick_range: Duration,
    elapsed: Cell<Duration>,
}

impl Clock {
    pub fn new(lifespan: Duration, tick_range: Duration) -> Self {
        Self {
            lifespan,
            tick_range,
            elapsed: Cell::new(Self::initial_duration()),
        }
    }

    fn initial_duration() -> Duration {
        Duration::from_secs(0)
    }

    fn reset(&self) {
        self.elapsed.set(Self::initial_duration());
    }

    fn tick(&self) {
        self.elapsed.set(self.elapsed.get() + self.tick_range);
    }

    fn is_done(&self) -> bool {
        self.elapsed.get() >= self.lifespan
    }
}

#[derive(Debug)]
enum Signal {
    Resume,
    Pause,
}

pub struct Pomodoro {
    working: Clock,
    short_break: Clock,
    long_break: Clock,
    long_break_interval: u8,
    counter: Counter,
    continuous: bool,
    until: Option<u8>,
    current_status: Phase,
    shared: Shared,
}

impl Pomodoro {
    pub fn new(
        working: Clock,
        short_break: Clock,
        long_break: Clock,
        long_break_interval: u8,
        continuous: bool,
        until: Option<u8>,
    ) -> Self {
        Self {
            working: working,
            short_break,
            long_break,
            long_break_interval,
            counter: Counter::new(),
            continuous,
            until,
            current_status: Phase::Working,
            shared: Shared::new(),
        }
    }

    fn is_consumed(&self) -> bool {
        self.until
            .map(|u| self.counter.working >= u)
            .unwrap_or(false)
    }

    fn current_status(&self) -> Phase {
        self.current_status.clone()
    }

    fn current_timer(&self) -> &Clock {
        match self.current_status() {
            Phase::Working => &self.working,
            Phase::ShortBreak => &self.short_break,
            Phase::LongBreak =>  &self.long_break,
        }
    }

    fn increment_current_status_counter(&mut self) {
        match self.current_status() {
            Phase::Working => self.counter.increment_working(),
            Phase::ShortBreak => self.counter.increment_short_break(),
            Phase::LongBreak => self.counter.increment_long_break(),
        };
    }

    fn is_reached_long_break(&self) -> bool {
        let v = self.counter.working;
        v > 0 && v % self.long_break_interval == 0
    }

    fn next_status(&mut self) -> Phase {
        if !self.current_timer().is_done() {
            return self.current_status();
        } else if self.current_status() != Phase::LongBreak && self.is_reached_long_break() {
            return Phase::LongBreak;
        }
        match self.current_status() {
            Phase::Working => Phase::ShortBreak,
            Phase::ShortBreak => Phase::Working,
            Phase::LongBreak => Phase::Working,
        }
    }

    fn is_active(&self) -> bool {
        let paused = self.shared.paused.lock().unwrap();
        !*paused
    }

    fn next_cycle(&mut self) {
        self.increment_current_status_counter();
        let next_status = self.next_status();
        self.current_timer().reset();
        self.current_status = next_status;
    }

    fn proceed(&self) {
        self.current_timer().tick();
    }

    async fn wait(&self) {
        let tick = self.current_timer().tick_range;
        sleep(tick).await;
        self.proceed();
    }

    pub async fn run(&mut self) {
        self.shared.resume();
        while !self.is_consumed() && self.is_active() {
            if !self.current_timer().is_done() {
                self.wait().await;
                continue;
            }
            self.next_cycle();
            if !self.continuous {
                self.shared.pause();
            }
        };
    }
}

#[test]
fn timer_struct() {
    let t = Clock::new(Duration::from_secs(2), Duration::from_secs(1));
    assert_eq!(t.elapsed.get(), Clock::initial_duration());
    t.tick();
    assert!(!t.is_done());
    assert_eq!(t.elapsed.get(), t.tick_range);
    t.tick();
    assert!(t.is_done());
    t.reset();
    assert_eq!(t.elapsed.get(), Clock::initial_duration());
    assert!(!t.is_done());
}

#[test]
fn pomodoro_timer_works_fine() {
    let working_timer = Clock::new(Duration::from_micros(1), Duration::from_micros(1));
    let short_break_timer = Clock::new(Duration::from_micros(1), Duration::from_micros(1));
    let long_break_timer = Clock::new(Duration::from_micros(1), Duration::from_micros(1));
    let mut pomodoro = Pomodoro::new(
        working_timer,
        short_break_timer,
        long_break_timer,
        2,
        true,
        Some(3),
    );

    assert_eq!(pomodoro.current_status(), Phase::Working);
    assert_eq!(pomodoro.next_status(), Phase::Working);
    assert!(!pomodoro.is_reached_long_break());
    pomodoro.proceed();
    assert!(pomodoro.current_timer().is_done());
    assert_eq!(pomodoro.next_status(), Phase::ShortBreak);
    pomodoro.next_cycle();
    assert_eq!(pomodoro.counter.working, 1);
    assert_eq!(pomodoro.current_status(), Phase::ShortBreak);
    pomodoro.proceed();
    pomodoro.next_cycle();
    assert!(!pomodoro.is_reached_long_break());
    pomodoro.proceed();
    pomodoro.next_cycle();
    assert_eq!(pomodoro.current_status(), Phase::LongBreak);
    pomodoro.proceed();
    pomodoro.next_cycle();
    assert_eq!(pomodoro.current_status(), Phase::Working);
    assert!(!pomodoro.is_consumed());
    pomodoro.proceed();
    pomodoro.next_cycle();
    assert!(pomodoro.is_consumed());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn trasition() {
    let working_timer = Clock::new(Duration::from_micros(2), Duration::from_micros(1));
    let short_break_timer = Clock::new(Duration::from_micros(3), Duration::from_micros(1));
    let long_break_timer = Clock::new(Duration::from_micros(4), Duration::from_micros(1));
    let mut pomodoro = Pomodoro::new(
        working_timer,
        short_break_timer,
        long_break_timer,
        2,
        true,
        Some(3),
    );
    pomodoro.run().await;
    assert!(pomodoro.is_consumed());
    assert_eq!(pomodoro.counter.working, 3);
    assert_eq!(pomodoro.counter.short_break, 1);
    assert_eq!(pomodoro.counter.long_break, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn continuous_option_false() {
    let working_timer = Clock::new(Duration::from_micros(1), Duration::from_micros(1));
    let short_break_timer = Clock::new(Duration::from_micros(1), Duration::from_micros(1));
    let long_break_timer = Clock::new(Duration::from_micros(1), Duration::from_micros(1));
    let mut pomodoro = Pomodoro::new(
        working_timer,
        short_break_timer,
        long_break_timer,
        2,
        false,
        None,
   );
    pomodoro.run().await;
    assert!(!pomodoro.is_active());
    assert_eq!(pomodoro.counter.working, 1);
    assert_eq!(pomodoro.counter.short_break, 0);
    assert_eq!(pomodoro.counter.long_break, 0);
}

// #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
// async fn runtime() {
//     let working_timer = Clock::new(Duration::from_micros(1), Duration::from_micros(1));
//     let short_break_timer = Clock::new(Duration::from_micros(1), Duration::from_micros(1));
//     let long_break_timer = Clock::new(Duration::from_micros(1), Duration::from_micros(1));
//     let mut pomodoro = Pomodoro::new(
//         working_timer,
//         short_break_timer,
//         long_break_timer,
//         2,
//         true,
//         Some(100),
//     );
//     let (sender, receiver) = mpsc::channel::<Signal>();
//     let task1 = tokio::spawn(async move {
//         pomodoro.run().await;
//     });
//     let task2 = tokio::spawn(async move {
//         sleep(Duration::from_micros(1)).await;
//         let result = sender.send(Signal::Pause);
//         assert!(result.is_ok());
//     });
//     tokio::join!(task1, task2);
// }