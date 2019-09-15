extern crate chrono;
extern crate timer;
use chrono::Duration;
use std::sync::{Arc, Mutex};
use timer::Guard;
use timer::Timer;

struct DebounceData<A, B: FnMut(A) + Send + Sync + 'static> {
    value: A,
    timer: Timer,
    timer_guard: Option<Guard>,
    callback: B,
}

pub fn closure<A: Eq + Copy + Send + Sync + 'static, B: FnMut(A) + Send + Sync + 'static>(
    default_value: A,
    f: B,
) -> Box<dyn Fn(A) -> () + Send + Sync> {
    let bounce_time: Duration = Duration::milliseconds(10);
    let state = Arc::new(Mutex::new(DebounceData {
        value: default_value,
        timer: Timer::new(),
        timer_guard: None,
        callback: f,
    }));
    return Box::new(move |new_value| {
        let mut data = state.lock().unwrap();
        if data.value != new_value {
            if new_value != default_value {
                data.value = new_value;
                match &mut data.timer_guard {
                    Some(guard) => drop(guard),
                    None => (&mut data.callback)(new_value),
                }
                data.timer_guard = None;
            } else {
                data.value = new_value;
                let state_ref = state.clone();
                let guard = data.timer.schedule_with_delay(bounce_time, move || {
                    let mut data = state_ref.lock().unwrap();
                    data.timer_guard = None;
                    (&mut data.callback)(new_value);
                });
                data.timer_guard = Some(guard);
            }
        }
    });
}
