use std::sync::{Arc, Mutex};
use std::cell::Cell;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::sleep;

type Shared<T> = Arc<Mutex<T>>;


#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Status {
    Working,
    ShortBreak,
    LongBreak,
}

#[derive(Debug, Clone)]
struct Counter {
    working: Shared<Cell<u8>>,
    short_break: Shared<Cell<u8>>,
    long_break: Shared<Cell<u8>>,
}

impl Counter {
    fn new() -> Self {
        Self {
            working: Arc::new(Mutex::new(Cell::new(0))),
            short_break: Arc::new(Mutex::new(Cell::new(0))),
            long_break: Arc::new(Mutex::new(Cell::new(0))),
        }
    }

    fn increment_working(&self) {
        let lock = self.working.lock().unwrap();
        lock.set(lock.get() + 1);
    }

    fn increment_short_break(&self) {
        let lock = self.short_break.lock().unwrap();
        lock.set(lock.get() + 1);
    }

    fn increment_long_break(&self) {
        let lock = self.long_break.lock().unwrap();
        lock.set(lock.get() + 1)
    }

    fn current_working(&self) -> u8 {
        let lock = self.working.lock().unwrap();
        lock.get()
    }

    fn current_short_break(&self) -> u8 {
        let lock = self.short_break.lock().unwrap();
        lock.get()
    }

    fn current_long_break(&self) -> u8 {
        let lock = self.long_break.lock().unwrap();
        lock.get()
    }
}

#[derive(Debug, Clone)]
pub struct Timer {
    lifespan: Duration,
    tick_range: Duration,
    elapsed: Shared<Cell<Duration>>,
}

impl Timer {
    pub fn new(lifespan: Duration, tick_range: Duration) -> Self {
        Self {
            lifespan,
            tick_range,
            elapsed: Arc::new(Mutex::new(Cell::new(Self::initial_duration()))),
        }
    }

    fn initial_duration() -> Duration {
        Duration::from_secs(0)
    }

    fn reset(&self) {
        let lock = self.elapsed.lock().unwrap();
        lock.set(Self::initial_duration());
    }

    fn tick(&self) {
        let lock = self.elapsed.lock().unwrap();
        lock.set(lock.get() + self.tick_range);
    }

    fn is_done(&self) -> bool {
        let lock = self.elapsed.lock().unwrap();
        lock.get() >= self.lifespan
    }

    fn current_elapsed(&self) -> Duration {
        let lock = self.elapsed.lock().unwrap();
        lock.get()
    }
}

#[derive(Debug)]
enum Signal {
    Resume,
    Pause,
}

struct InnerState {
    current_status: Mutex<Cell<Status>>,
    paused: Mutex<Cell<bool>>,
}

impl InnerState {
    fn reveal_current_state(&self) -> Status {
        self.current_status.lock().unwrap().get()
    }

    fn reveal_paused(&self) -> bool {
        self.paused.lock().unwrap().get()
    }
}

pub struct Pomodoro {
    working: Timer,
    short_break: Timer,
    long_break: Timer,
    long_break_interval: u8,
    counter: Counter,
    continuous: bool,
    until: Option<u8>,
    inner_state: InnerState,
}

impl Pomodoro {
    pub fn new(
        working: Timer,
        short_break: Timer,
        long_break: Timer,
        long_break_interval: u8,
        continuous: bool,
        until: Option<u8>,
    ) -> Self {
        let inner_state = InnerState {
            current_status: Mutex::new(Cell::new(Status::Working)),
            paused: Mutex::new(Cell::new(true)),
        };
        Self {
            working: working,
            short_break,
            long_break,
            long_break_interval,
            counter: Counter::new(),
            continuous,
            until,
            inner_state,
        }
    }

    fn is_consumed(&self) -> bool {
        self.until
            .map(|u| self.counter.current_working() >= u)
            .unwrap_or(false)
    }

    fn current_status(&self) -> Status {
        self.inner_state.reveal_current_state()
    }

    fn paused(&self) -> bool {
        self.inner_state.reveal_paused()
    }

    fn current_timer(&self) -> &Timer {
        match self.current_status() {
            Status::Working => &self.working,
            Status::ShortBreak => &self.short_break,
            Status::LongBreak => &self.long_break,
        }
    }

    fn increment_current_status_counter(&self) {
        match self.current_status() {
            Status::Working => self.counter.increment_working(),
            Status::ShortBreak => self.counter.increment_short_break(),
            Status::LongBreak => self.counter.increment_long_break(),
        };
    }

    fn reached_long_break(&self) -> bool {
        let v = self.counter.current_working();
        v > 0 && v % self.long_break_interval == 0
    }

    fn next_status(&self) -> Status {
        if !self.current_timer().is_done() {
            return self.current_status();
        } else if self.current_status() != Status::LongBreak && self.reached_long_break() {
            return Status::LongBreak;
        }
        match self.current_status() {
            Status::Working => Status::ShortBreak,
            Status::ShortBreak => Status::Working,
            Status::LongBreak => Status::Working,
        }
    }

    fn is_active(&self) -> bool {
        !self.paused()
    }

    pub fn resume(&self) {
        let paused = self.inner_state.paused.lock().unwrap();
        paused.set(false);
    }

    pub fn pause(&self) {
        let paused = self.inner_state.paused.lock().unwrap();
        paused.set(true);
    }

    fn next_cycle(&self) {
        self.increment_current_status_counter();
        let next_status = self.next_status();
        self.current_timer().reset();
        let current_status = self.inner_state.current_status.lock().unwrap();
        current_status.set(next_status);
    }

    fn proceed(&self) {
        self.current_timer().tick();
    }

    pub async fn run(&self) {
        self.resume();
        while !self.is_consumed() && self.is_active() {
            if !self.current_timer().is_done() {
                sleep(self.current_timer().tick_range).await;
                self.proceed();
                continue;
            }
            self.next_cycle();
            if !self.continuous {
                self.pause();
            }
        }
    }
}

struct Client {
    transmitter: mpsc::Sender<Signal>,
}

async fn start(pomodoro: Pomodoro) -> Client {
    let (tx, mut rx) = mpsc::channel::<Signal>(2); // buffer is adhoc.
    tokio::spawn(async move {
        while let Some(signal) = rx.recv().await {
            match signal {
                Signal::Resume => pomodoro.resume(),
                Signal::Pause => pomodoro.pause(),
            };
        }
        pomodoro.run().await;
    });
    Client { transmitter: tx }
}

#[test]
fn timer_struct() {
    let t = Timer::new(Duration::from_secs(2), Duration::from_secs(1));
    assert_eq!(t.current_elapsed(), Timer::initial_duration());
    t.tick();
    assert!(!t.is_done());
    assert_eq!(t.current_elapsed(), t.tick_range);
    t.tick();
    assert!(t.is_done());
    t.reset();
    assert_eq!(t.current_elapsed(), Timer::initial_duration());
    assert!(!t.is_done());
}

#[test]
fn pomodoro_timer_works_fine() {
    let working_timer = Timer::new(Duration::from_micros(1), Duration::from_micros(1));
    let short_break_timer = Timer::new(Duration::from_micros(1), Duration::from_micros(1));
    let long_break_timer = Timer::new(Duration::from_micros(1), Duration::from_micros(1));
    let pomodoro = Pomodoro::new(
        working_timer,
        short_break_timer,
        long_break_timer,
        2,
        true,
        Some(3),
    );

    assert_eq!(pomodoro.current_status(), Status::Working);
    assert_eq!(pomodoro.next_status(), Status::Working);
    assert!(!pomodoro.reached_long_break());
    pomodoro.proceed();
    assert!(pomodoro.current_timer().is_done());
    assert_eq!(pomodoro.next_status(), Status::ShortBreak);
    pomodoro.next_cycle();
    assert_eq!(pomodoro.counter.current_working(), 1);
    assert_eq!(pomodoro.current_status(), Status::ShortBreak);
    pomodoro.proceed();
    pomodoro.next_cycle();
    assert!(!pomodoro.reached_long_break());
    pomodoro.proceed();
    pomodoro.next_cycle();
    assert_eq!(pomodoro.current_status(), Status::LongBreak);
    pomodoro.proceed();
    pomodoro.next_cycle();
    assert_eq!(pomodoro.current_status(), Status::Working);
    assert!(!pomodoro.is_consumed());
    pomodoro.proceed();
    pomodoro.next_cycle();
    assert!(pomodoro.is_consumed());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn trasition() {
    let working_timer = Timer::new(Duration::from_micros(2), Duration::from_micros(1));
    let short_break_timer = Timer::new(Duration::from_micros(3), Duration::from_micros(1));
    let long_break_timer = Timer::new(Duration::from_micros(4), Duration::from_micros(1));
    let pomodoro = Pomodoro::new(
        working_timer,
        short_break_timer,
        long_break_timer,
        2,
        true,
        Some(3),
    );
    pomodoro.run().await;
    assert!(pomodoro.is_consumed());
    assert_eq!(pomodoro.counter.current_working(), 3);
    assert_eq!(pomodoro.counter.current_short_break(), 1);
    assert_eq!(pomodoro.counter.current_long_break(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn continuous_option_false() {
    let working_timer = Timer::new(Duration::from_micros(1), Duration::from_micros(1));
    let short_break_timer = Timer::new(Duration::from_micros(1), Duration::from_micros(1));
    let long_break_timer = Timer::new(Duration::from_micros(1), Duration::from_micros(1));
    let pomodoro = Pomodoro::new(
        working_timer,
        short_break_timer,
        long_break_timer,
        2,
        false,
        None,
    );
    pomodoro.run().await;
    assert!(!pomodoro.is_active());
    assert_eq!(pomodoro.counter.current_working(), 1);
    assert_eq!(pomodoro.counter.current_short_break(), 0);
    assert_eq!(pomodoro.counter.current_long_break(), 0);
}

// #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn execution() {
    let working_timer = Timer::new(Duration::from_secs(1), Duration::from_secs(1));
    let short_break = Timer::new(Duration::from_secs(1), Duration::from_secs(1));
    let long_break = Timer::new(Duration::from_secs(1), Duration::from_secs(1));
    let pomodoro = Pomodoro::new(
        working_timer,
        short_break,
        long_break,
        1,
        true,
        Some(3),
    );
    pomodoro.run().await;
    sleep(Duration::from_secs(2)).await;
    pomodoro.pause();
    assert_eq!(pomodoro.counter.current_working(), 2);
}