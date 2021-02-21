use std::{cell::Cell, ops::DerefMut};
use std::time::Duration;

use tokio::time::sleep;

#[derive(Clone, PartialEq, Eq, Debug)]
enum Status {
    Working,
    ShortBreak,
    LongBreak,
}

#[derive(PartialEq, Eq, Debug)]
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
        self.long_break.set(self.long_break.get() + 1)
    }
}

#[derive(Debug, Clone)]
pub struct Timer {
    lifespan: Duration,
    tick_range: Duration,
    elapsed: Cell<Duration>,
}

impl Timer {
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

pub struct Pomodoro {
    working: Timer,
    short_break: Timer,
    long_break: Timer,
    long_break_interval: u8,
    current_status: Status,
    counter: Counter,
    paused: bool,
    continuous: bool,
    until: Option<u8>,
}

impl Pomodoro {
    pub fn new(
        working: Timer,
        short_break: Timer,
        long_break: Timer,
        long_break_interval: u8,
        until: Option<u8>,
    ) -> Self {
        Self {
            working: working,
            short_break,
            long_break,
            long_break_interval,
            current_status: Status::Working,
            counter: Counter::new(),
            paused: true,
            continuous: true, // TODO: Implemet for auto trasition.
            until,
        }
    }

    fn is_consumed(&self) -> bool {
        self.until
            .map(|u| self.counter.working.get() >= u)
            .unwrap_or(false)
    }

    fn current_timer(&self) -> &Timer {
        match self.current_status {
            Status::Working => &self.working,
            Status::ShortBreak => &self.short_break,
            Status::LongBreak => &self.long_break,
        }
    }

    fn increment_current_status_counter(&self) {
        match self.current_status {
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
            return self.current_status.clone();
        } else if self.current_status != Status::LongBreak && self.reached_long_break() {
            return Status::LongBreak;
        }
        match self.current_status {
            Status::Working => Status::ShortBreak,
            Status::ShortBreak => Status::Working,
            Status::LongBreak => Status::Working,
        }
    }

    fn is_active(&self) -> bool {
        !self.paused
    }

    async fn start(&mut self) {
        self.resume();
        self.run().await;
    }

    fn resume(&mut self) {
        self.paused = false;
    }

    fn pause(&mut self) {
        self.paused = true;
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

    async fn run(&mut self) {
        while !self.is_consumed() && self.is_active() {
            if !self.current_timer().is_done() {
                sleep(self.current_timer().tick_range).await;
                self.proceed();
            } else {
                self.next_cycle();
            }
        }
    }
}

#[test]
fn timer_struct() {
    let t = Timer::new(Duration::from_secs(2), Duration::from_secs(1));
    assert_eq!(t.elapsed, Cell::new(Timer::initial_duration()));
    t.tick();
    assert!(!t.is_done());
    assert_eq!(t.elapsed, Cell::new(t.tick_range));
    t.tick();
    assert!(t.is_done());
    t.reset();
    assert_eq!(t.elapsed, Cell::new(Timer::initial_duration()));
    assert!(!t.is_done());
}

#[test]
fn pomodoro_timer_works_fine() {
    let working_timer = Timer::new(Duration::from_micros(1), Duration::from_micros(1));
    let short_break_timer = Timer::new(Duration::from_micros(1), Duration::from_micros(1));
    let long_break_timer = Timer::new(Duration::from_micros(1), Duration::from_micros(1));
    let mut pomodoro = Pomodoro::new(
        working_timer,
        short_break_timer,
        long_break_timer,
        2,
        Some(3),
    );

    assert_eq!(pomodoro.current_status, Status::Working);
    assert_eq!(pomodoro.next_status(), Status::Working);
    assert!(!pomodoro.reached_long_break());
    pomodoro.proceed();
    assert!(pomodoro.current_timer().is_done());
    assert_eq!(pomodoro.next_status(), Status::ShortBreak);
    pomodoro.next_cycle();
    assert_eq!(pomodoro.counter.working.get(), 1);
    assert_eq!(pomodoro.current_status, Status::ShortBreak);
    pomodoro.proceed();
    pomodoro.next_cycle();
    assert!(!pomodoro.reached_long_break());
    pomodoro.proceed();
    pomodoro.next_cycle();
    assert_eq!(pomodoro.current_status, Status::LongBreak);
    pomodoro.proceed();
    pomodoro.next_cycle();
    assert_eq!(pomodoro.current_status, Status::Working);
    assert!(!pomodoro.is_consumed());
    pomodoro.proceed();
    pomodoro.next_cycle();
    assert!(pomodoro.is_consumed());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn run_continuous_loop() {
    let working_timer = Timer::new(Duration::from_micros(2), Duration::from_micros(1));
    let short_break_timer = Timer::new(Duration::from_micros(3), Duration::from_micros(1));
    let long_break_timer = Timer::new(Duration::from_micros(4), Duration::from_micros(1));
    let mut pomodoro = Pomodoro::new(
        working_timer,
        short_break_timer,
        long_break_timer,
        2,
        Some(3),
    );
    pomodoro.start().await;
    assert!(pomodoro.is_consumed());
    assert_eq!(pomodoro.counter.working.get(), 3);
    assert_eq!(pomodoro.counter.short_break.get(), 1);
    assert_eq!(pomodoro.counter.long_break.get(), 1);
}