use std::sync::Mutex;
use std::cell::Cell;
use std::time::Duration;

use tokio::time::sleep;


#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Status {
    Working,
    ShortBreak,
    LongBreak,
}

#[derive(Debug, Clone)]
struct Counter {
    working: Cell<u8>,
    short_break: Cell<u8>,
    long_break: Cell<u8>,
}

impl Counter {
    fn new() -> Self {
        Self {
            working: Cell::new(0),
            short_break: Cell::new(0),
            long_break: Cell::new(0),
        }
    }

    fn increment_working(&self) {
        self.working.set(self.working.get() + 1);
    }

    fn increment_short_break(&self) {
        self.short_break.set(self.short_break.get() + 1);
    }

    fn increment_long_break(&self) {
        self.long_break.set(self.long_break.get() + 1);
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
    working: Clock,
    short_break: Clock,
    long_break: Clock,
    long_break_interval: u8,
    counter: Counter,
    continuous: bool,
    until: Option<u8>,
    inner_state: InnerState,
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
            .map(|u| self.counter.working.get() >= u)
            .unwrap_or(false)
    }

    fn current_status(&self) -> Status {
        self.inner_state.reveal_current_state()
    }

    fn paused(&self) -> bool {
        self.inner_state.reveal_paused()
    }

    fn current_timer(&self) -> &Clock {
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
        let v = self.counter.working.get();
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
                let tick = self.current_timer().tick_range;
                tokio::spawn(async move {
                    sleep(tick).await;
                });
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
    assert_eq!(pomodoro.counter.working.get(), 1);
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
    let working_timer = Clock::new(Duration::from_micros(2), Duration::from_micros(1));
    let short_break_timer = Clock::new(Duration::from_micros(3), Duration::from_micros(1));
    let long_break_timer = Clock::new(Duration::from_micros(4), Duration::from_micros(1));
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
    assert_eq!(pomodoro.counter.working.get(), 3);
    assert_eq!(pomodoro.counter.short_break.get(), 1);
    assert_eq!(pomodoro.counter.long_break.get(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn continuous_option_false() {
    let working_timer = Clock::new(Duration::from_micros(1), Duration::from_micros(1));
    let short_break_timer = Clock::new(Duration::from_micros(1), Duration::from_micros(1));
    let long_break_timer = Clock::new(Duration::from_micros(1), Duration::from_micros(1));
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
    assert_eq!(pomodoro.counter.working.get(), 1);
    assert_eq!(pomodoro.counter.short_break.get(), 0);
    assert_eq!(pomodoro.counter.long_break.get(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn execution() {
    let working_timer = Clock::new(Duration::from_micros(1), Duration::from_micros(1));
    let short_break = Clock::new(Duration::from_micros(1), Duration::from_micros(1));
    let long_break = Clock::new(Duration::from_micros(1), Duration::from_micros(1));
    let pomodoro = Pomodoro::new(
        working_timer,
        short_break,
        long_break,
        10,
        true,
        Some(10),
    );
    pomodoro.run().await;
    sleep(Duration::from_micros(3)).await;
    pomodoro.pause();
    assert!(!pomodoro.is_active());
    assert_eq!(pomodoro.counter.working.get(), 3);
}