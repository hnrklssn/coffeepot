use rppal::pwm::{Channel, Pwm, Polarity};
use std::thread;
use std::time::Duration;
use std::io::{stdin,stdout,Write};
use std::error::Error;
mod coffeepot;
mod debounce;
use coffeepot::{Coffeepot, PotState};
use chrono::prelude::*;
use rppal::gpio::Gpio;
use rppal::gpio::Level;
use std::sync::mpsc::{channel, Receiver, Sender, RecvTimeoutError};

enum Action {
    Start,
    Stop,
}

fn pwm_cycle_wait(rx: &Receiver<Action>) {
    match rx.recv() {
        Ok(Action::Start) => (),
        _ => pwm_cycle_wait(rx),
    }
}

fn cycle_pwm(pwm: Channel, rx: Receiver<Action>) -> Result<(), Box<dyn Error>> {
	let led = Pwm::with_period(pwm, Duration::from_millis(5), Duration::from_millis(2), Polarity::Normal, true)?;
	println!("pwm led: {:?}", led);
	loop {
		for x in (0..100).chain((0..100).rev()) {
			led.set_duty_cycle((x as f64)/100.0)?;
			println!("{}", x);
			match rx.recv_timeout(Duration::from_millis(10)) {
                            Ok(Action::Stop) => pwm_cycle_wait(&rx),
                            Err(RecvTimeoutError::Timeout) => (),
                            Err(_) => (), // TODO: kill thread
                            _ => (),
                        }
		}
	}
	Ok(())
}

fn ready_pwm_init(pwm: Channel) -> Sender<Action> {
    let (tx, rx) = channel();
    thread::spawn(move || cycle_pwm(pwm, rx).unwrap());
    tx
}

fn demo() -> Result<(), Box<dyn Error>> {
	println!("Hello, world!");
        let coffeepot = Coffeepot::new(&|new_state| println!("state changed to {:?}", new_state));
        loop {
            // Need empty string at start of every loop iteration, read_line appends
            let mut input = String::new();
            stdin().read_line(&mut input).expect("Did not enter a correct string");
            match input.trim().as_ref() {
                "a" => coffeepot.activate(chrono::Duration::seconds(2)),
                "i" => coffeepot.inactivate(),
                "r" => coffeepot.toggle_ready(),
                "d" => coffeepot.activate_delayed(chrono::Duration::seconds(5), Local::now() + FixedOffset::east(5)),
                other => println!("unexpected input: {}", other),
            }
            println!("state: {:?}", coffeepot.current_state());
        }
        Ok(())
}

// Gpio uses BCM pin numbering
const GPIO_READY_PIN: u8 = 17;
const GPIO_POWER_PIN: u8 = 20;
const PWM_READY_LED_PIN: Channel = Channel::Pwm0;
const PWM_POWER_LED_PIN: Channel = Channel::Pwm1;


fn main() -> Result<(), Box<dyn Error>> {
	println!("Hello, world!");
        let mut ready_input = Gpio::new()?.get(GPIO_READY_PIN)?.into_input_pulldown();
        let mut power_input = Gpio::new()?.get(GPIO_POWER_PIN)?.into_input_pulldown();
    
        let tx = ready_pwm_init(PWM_READY_LED_PIN);
        //let tx = ready_pwm_init(PWM_POWER_LED_PIN);

        let coffeepot = Coffeepot::new(move |new_state| {
            println!("state changed to {:?}", new_state);
            let result = match new_state {
                PotState::Waiting => tx.send(Action::Start),
                _ => tx.send(Action::Stop),
            };
            match result {
                Err(s) => println!("error transmitting: {}", s),
                _ => (),
            }
        });
        let pot2 = coffeepot.clone();
        let pot3 = coffeepot.clone();
        let update_ready = debounce::closure(Level::Low, move |level| { println!("level: {}", level); if level == Level::High { pot2.toggle_ready();}});
        let update_power = debounce::closure(Level::Low, move |level| if level == Level::High {
            pot3.toggle_active();
        });
        ready_input.set_async_interrupt(rppal::gpio::Trigger::Both, update_ready)?;
        power_input.set_async_interrupt(rppal::gpio::Trigger::Both, update_power)?;
        let mut exit = false;
        while !exit {
            // Need empty string at start of every loop iteration, read_line appends
            let mut input = String::new();
            stdin().read_line(&mut input).expect("Did not enter a correct string");
            match input.trim().as_ref() {
                "a" => coffeepot.activate(chrono::Duration::seconds(2)),
                "i" => coffeepot.inactivate(),
                "r" => coffeepot.toggle_ready(),
                "d" => coffeepot.activate_delayed(chrono::Duration::seconds(5), Local::now() + FixedOffset::east(5)),
                "e" => exit = true,
                other => println!("unexpected input: {}", other),
            }
            println!("state: {:?}", coffeepot.current_state());
        }
        println!("exiting");
        Ok(())
}
