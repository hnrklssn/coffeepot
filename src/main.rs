use rppal::pwm::{Channel, Pwm, Polarity};
use std::thread;
//use std::time::Duration;
use std::io::{stdin,stdout,Write};
use std::error::Error;
mod coffeepot;
mod debounce;
use coffeepot::Coffeepot;
use chrono::prelude::*;
use rppal::gpio::Gpio;
use rppal::gpio::Level;

/*fn cycle_pwm() -> Result<(), Box<dyn Error>> {
	let led = Pwm::with_period(Channel::Pwm0, Duration::from_millis(5), Duration::from_millis(2), Polarity::Normal, true)?;
	println!("pwm led: {:?}", led);
	loop {
		for x in (0..100).chain((0..100).rev()) {
			led.set_duty_cycle((x as f64)/100.0)?;
			println!("{}", x);
			thread::sleep(Duration::from_millis(10));
		}
	}
	Ok(())
}*/

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


fn main() -> Result<(), Box<dyn Error>> {
	println!("Hello, world!");
        let mut ready_input = Gpio::new()?.get(GPIO_READY_PIN)?.into_input_pulldown();
        let coffeepot = Coffeepot::new(&|new_state| println!("state changed to {:?}", new_state));
        let pot2 = coffeepot.clone();
        let update_func = debounce::closure(Level::Low, move |level| { println!("level: {}", level); pot2.toggle_ready();});
        ready_input.set_async_interrupt(rppal::gpio::Trigger::Both, update_func)?;
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
