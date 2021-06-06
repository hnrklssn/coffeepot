extern crate chrono;
extern crate timer;
use chrono::DateTime;
use chrono::Duration;
use chrono::TimeZone;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use timer::Guard;

#[derive(PartialEq, Copy, Clone, Debug)]
#[repr(u8)]
pub enum PotState {
    Idle = 1,
    Ready = 2,
    Waiting = 3,
    Active = 4,
    Shutdown = 5,
}

struct CoffeepotInternals {
    state: PotState,
    timer_guard: Option<Guard>,
    clock: timer::Timer,
    tx: Sender<PotState>,
}

impl CoffeepotInternals {
    fn cancel_timer(&mut self) {
        match &self.timer_guard {
            Some(guard) => {
                debug!("cancelling coffeepot timer");
                drop(guard);
                self.timer_guard = None;
            }
            _ => (),
        }
    }

    fn change_state(&mut self, new_state: PotState) {
        debug!("changing coffeepot state to {:?}", new_state);
        // Cancel timer even if new state is the same, since a new timer will be
        // instantiated if there is currently one
        self.cancel_timer();
        if new_state == self.state {
            return;
        }
        self.state = new_state;
        self.tx.send(new_state).unwrap();
    }
}

/**
 * Keeps the callback function to a single thread, and isolates the callback
 * type from polluting all of the structs with trait bounds.
 * Not strictly needed.
 */
fn callback_handler<B: FnMut(PotState) + Send + 'static>(
    mut cb: B,
) -> (Sender<PotState>, thread::JoinHandle<()>) {
    let (tx, rx) = channel();
    (
        tx,
        thread::spawn(move || loop {
            let state = rx.recv().unwrap();
            debug!("received coffeepot state {:?}", state);
            cb(state);
            if state == PotState::Shutdown {
                info!("exiting coffeepot callback handler loop");
                break;
            }
        }),
    )
}

pub struct Coffeepot {
    props: Arc<Mutex<CoffeepotInternals>>,
}

impl Coffeepot {
    pub fn new<B: FnMut(PotState) + Send + 'static>(cb: B) -> Self {
        let (tx, _) = callback_handler(cb);
        // send initial message with the starting state
        tx.send(PotState::Idle)
            .map_err(|_| error!("error sending initial idle state"))
            .expect("error sending initial idle state");
        let pot = CoffeepotInternals {
            state: PotState::Idle,
            timer_guard: None,
            clock: timer::Timer::new(),
            tx,
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
        debug!("fetching current state");
        self.props.lock().unwrap().state
    }

    pub fn activate(&self, time: Duration) {
        let mut attrs = self.props.lock().unwrap();
        info!("activating for {}", time);
        attrs.change_state(PotState::Active);
        let clone = self.clone();
        let guard = attrs
            .clock
            .schedule_with_delay(time, move || clone.inactivate());
        attrs.timer_guard = Some(guard);
    }

    pub fn activate_delayed<Tz: TimeZone>(&self, time: Duration, activation_time: DateTime<Tz>) {
        let mut attrs = self.props.lock().unwrap();
        if attrs.state != PotState::Ready && attrs.state != PotState::Waiting {
            debug!("got activate delayed in non-ready state");
            return;
        }
        attrs.change_state(PotState::Waiting);
        info!("activation time set to {:#?}", activation_time);
        let clone = self.clone();
        let guard = attrs
            .clock
            .schedule_with_date(activation_time, move || clone.activate(time));
        attrs.timer_guard = Some(guard);
    }

    pub fn inactivate(&self) {
        let mut attrs = self.props.lock().unwrap();
        info!("inactivating");
        attrs.change_state(PotState::Idle);
    }

    pub fn toggle_ready(&self) {
        let mut attrs = self.props.lock().unwrap();
        info!("toggling ready");
        match attrs.state {
            PotState::Idle => {
                attrs.change_state(PotState::Ready);
            },
            PotState::Ready | PotState::Waiting => {
                attrs.change_state(PotState::Idle);
            },
            _ => warn!("ready toggle invalid in current state"),
        }
    }

    pub fn toggle_active(&self) {
        let mut attrs = self.props.lock().unwrap();
        info!("toggling active");
        match attrs.state {
            PotState::Active => attrs.change_state(PotState::Idle),
            _ => attrs.change_state(PotState::Active),
        }
    }
}
