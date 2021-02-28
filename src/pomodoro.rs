use std::sync::mpsc;
use std::cell::Cell;
use std::time::Duration;

use tokio::time::sleep;
use tokio::sync::oneshot;


#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Status {
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
    current_status: Status,
    paused: bool,
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
            current_status: Status::Working,
            paused: true,
        }
    }

    fn is_consumed(&self) -> bool {
        self.until
            .map(|u| self.counter.working >= u)
            .unwrap_or(false)
    }

    fn current_status(&self) -> Status {
        self.current_status.clone()
    }

    fn paused(&self) -> bool {
        self.paused
    }

    fn current_timer(&self) -> &Clock {
        match self.current_status() {
            Status::Working => &self.working,
            Status::ShortBreak => &self.short_break,
            Status::LongBreak =>  &self.long_break,
        }
    }

    fn increment_current_status_counter(&mut self) {
        match self.current_status() {
            Status::Working => self.counter.increment_working(),
            Status::ShortBreak => self.counter.increment_short_break(),
            Status::LongBreak => self.counter.increment_long_break(),
        };
    }

    fn is_reached_long_break(&self) -> bool {
        let v = self.counter.working;
        v > 0 && v % self.long_break_interval == 0
    }

    fn next_status(&mut self) -> Status {
        if !self.current_timer().is_done() {
            return self.current_status();
        } else if self.current_status() != Status::LongBreak && self.is_reached_long_break() {
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

    pub async fn run(&mut self, receiver: mpsc::Receiver<Signal>) {
        self.resume();
        while !self.is_consumed() && self.is_active() {
            if let Ok(signal) = receiver.recv() {
                match signal {
                    Signal::Pause => self.pause(),
                    Signal::Resume => self.resume(),
                };
            }
            if !self.current_timer().is_done() {
                let tick = self.current_timer().tick_range;
                sleep(tick).await;
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
    let mut pomodoro = Pomodoro::new(
        working_timer,
        short_break_timer,
        long_break_timer,
        2,
        true,
        Some(3),
    );

    assert_eq!(pomodoro.current_status(), Status::Working);
    assert_eq!(pomodoro.next_status(), Status::Working);
    assert!(!pomodoro.is_reached_long_break());
    pomodoro.proceed();
    assert!(pomodoro.current_timer().is_done());
    assert_eq!(pomodoro.next_status(), Status::ShortBreak);
    pomodoro.next_cycle();
    assert_eq!(pomodoro.counter.working, 1);
    assert_eq!(pomodoro.current_status(), Status::ShortBreak);
    pomodoro.proceed();
    pomodoro.next_cycle();
    assert!(!pomodoro.is_reached_long_break());
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
    let mut pomodoro = Pomodoro::new(
        working_timer,
        short_break_timer,
        long_break_timer,
        2,
        true,
        Some(3),
    );
    let (_, receiver) = mpsc::channel::<Signal>();
    pomodoro.run(receiver).await;
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
   let (_, receiver) = mpsc::channel::<Signal>();
    pomodoro.run(receiver).await;
    assert!(!pomodoro.is_active());
    assert_eq!(pomodoro.counter.working, 1);
    assert_eq!(pomodoro.counter.short_break, 0);
    assert_eq!(pomodoro.counter.long_break, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runtime() {
    let working_timer = Clock::new(Duration::from_micros(1), Duration::from_micros(1));
    let short_break_timer = Clock::new(Duration::from_micros(1), Duration::from_micros(1));
    let long_break_timer = Clock::new(Duration::from_micros(1), Duration::from_micros(1));
    let mut pomodoro = Pomodoro::new(
        working_timer,
        short_break_timer,
        long_break_timer,
        2,
        true,
        Some(100),
    );
    let (sender, receiver) = mpsc::channel::<Signal>();
    tokio::spawn(async move {
        pomodoro.run(receiver).await;
    });
    sleep(Duration::from_micros(1)).await;
    let result = sender.send(Signal::Pause);
    assert!(result.is_ok());
}