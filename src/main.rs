use rppal::pwm::{Channel, Pwm, Polarity};
use std::thread;
use std::time::Duration;

use std::error::Error;
mod coffeepot;

//fn main() {
fn main() -> Result<(), Box<dyn Error>> {
	println!("Hello, world!");
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
}
