extern crate timer;
extern crate chrono;
use timer::Guard;
use chrono::Duration;
use chrono::DateTime;
use chrono::TimeZone;
use std::sync::{Arc, Mutex};


#[derive(PartialEq, Copy, Clone, Debug)]
pub enum PotState {
    Idle,
    Active,
    Waiting,
    Ready,
}

struct CoffeepotInternals {
    state: PotState,
    timer_guard: Option<Guard>,
    clock: timer::Timer,
    state_callback: &'static (Fn(PotState) -> () + Send + Sync),
}

impl CoffeepotInternals {
    fn cancel_timer(&mut self) {
        match &self.timer_guard {
            Some(guard) => {
                drop(guard);
                self.timer_guard = None;
            },
            _ => (),
        }
    }

    fn change_state(&mut self, new_state: PotState) {
        // Cancel timer even if new state is the same, since a new timer will be
        // instantiated if there is currently one
        self.cancel_timer();
        if new_state == self.state {
            return;
        }
        self.state = new_state;
        (self.state_callback)(new_state);
    }
}

pub struct Coffeepot {
    props: Arc<Mutex<CoffeepotInternals>>,
}

impl Coffeepot {
    pub fn new(cb: &'static (Fn(PotState) -> () + Send + Sync)) -> Self {
        let pot = CoffeepotInternals {
            state: PotState::Idle,
            timer_guard: None,
            clock: timer::Timer::new(),
            state_callback: cb,
        };
        Coffeepot {
            props: Arc::new(Mutex::new(pot)),
        }
    }

    pub fn clone(&self) -> Self {
        Coffeepot {
            props: self.props.clone(),
        }
    }

    pub fn current_state(&self) -> PotState {
        self.props.lock().unwrap().state
    }

    pub fn activate(&self, time: Duration) {
        let mut attrs = self.props.lock().unwrap();
        attrs.change_state(PotState::Active);
        let clone = self.clone();
        let guard = attrs.clock.schedule_with_delay(time, move || clone.inactivate());
        attrs.timer_guard = Some(guard);
    }

    pub fn activate_delayed<Tz: TimeZone>(&self, time: Duration, activation_time: DateTime<Tz>) {
        let mut attrs = self.props.lock().unwrap();
        if attrs.state != PotState::Ready {
            return;
        }
        attrs.change_state(PotState::Waiting);
        let clone = self.clone();
        let guard = attrs.clock.schedule_with_date(activation_time, move || clone.activate(time));
        attrs.timer_guard = Some(guard);
    }

    pub fn inactivate(&self) {
        let mut attrs = self.props.lock().unwrap();
        attrs.change_state(PotState::Idle);
    }

    pub fn toggle_ready(&self) {
        let mut attrs = self.props.lock().unwrap();
        match attrs.state {
            PotState::Idle => {
                attrs.change_state(PotState::Ready);
            },
            PotState::Ready => {
                attrs.change_state(PotState::Idle);
            },
            _ => (),
        }
    }

}
