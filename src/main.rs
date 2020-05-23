mod coffeepot;
mod debounce;
use chrono::prelude::*;
use coffeepot::{Coffeepot, PotState};
use rumqtt::{MqttClient, MqttOptions, Notification, QoS, Receiver, ReconnectOptions};
use std::error::Error;
use std::io::stdin;
use std::thread;

fn init_mqtt(url: &str, port: u16) -> (MqttClient, Receiver<Notification>) {
    let reconnection_options = ReconnectOptions::Always(10);
    let mqtt_options = MqttOptions::new("test-coffeepot", url, port)
        .set_keep_alive(10)
        .set_inflight(3)
        .set_request_channel_capacity(10)
        .set_reconnect_opts(reconnection_options)
        .set_clean_session(false);

    let (mut mqtt_client, notifications) = MqttClient::start(mqtt_options).unwrap();
    mqtt_client
        .subscribe("coffeepot/actions", QoS::AtLeastOnce)
        .unwrap();
    (mqtt_client, notifications)
}

/** Allow actions to be injected from network for home automation */
fn handle_notifications(coffeepot: Coffeepot, notifications: Receiver<Notification>) {
    for notification in notifications {
        match notification {
            Notification::Publish(packet) => {
                if packet.payload.len() < 1 {
                    println!("payload empty!");
                }
                match packet.payload[0] as char {
                    'a' => coffeepot.activate(chrono::Duration::seconds(2)),
                    'i' => coffeepot.inactivate(),
                    'd' => coffeepot.activate_delayed(
                        chrono::Duration::seconds(5),
                        Local::now() + FixedOffset::east(5),
                    ),
                    other => println!("unexpected input: {}", other),
                }
                println!("state: {:?}", coffeepot.current_state());
            }
            _ => (),
        }
    }
}

/** Allows actions to be injected from the terminal for testing purposes */
fn demo(coffeepot: Coffeepot) -> Result<(), Box<dyn Error>> {
    let mut exit = false;
    while !exit {
        // Need empty string at start of every loop iteration, read_line appends
        let mut input = String::new();
        stdin()
            .read_line(&mut input)
            .expect("Did not enter a correct string");
        match input.trim().as_ref() {
            "a" => coffeepot.activate(chrono::Duration::seconds(2)),
            "i" => coffeepot.inactivate(),
            "r" => coffeepot.toggle_ready(),
            "d" => coffeepot.activate_delayed(
                chrono::Duration::seconds(5),
                Local::now() + FixedOffset::east(5),
            ),
            "e" => exit = true,
            other => println!("unexpected input: {}", other),
        }
        println!("state: {:?}", coffeepot.current_state());
    }
    Ok(())
}

/** This main function can be compiled and run on x86 machines to test the
 * state machine without any connected hardware */
#[allow(dead_code)]
#[cfg(not(target_arch = "arm"))]
fn main() -> Result<(), Box<dyn Error>> {
    println!("Hello, world!");
    let (tx, rx) = init_mqtt("test.mosquitto.org", 1883);
    let coffeepot = Coffeepot::new({
        let mut tx = tx;
        move |new_state: PotState| {
            println!("state changed to {:?}", new_state);
            tx.publish(
                "coffeepot/state",
                QoS::AtLeastOnce,
                false,
                vec![new_state as u8],
            )
            .expect("mqtt publish failed");
            println!("state changed to {:?}_____", new_state);
        }
    });
    thread::spawn({
        let coffeepot = coffeepot.clone();
        move || handle_notifications(coffeepot, rx)
    });
    demo(coffeepot)
}

/* ***** Pi hardware dependent stuff below here ******* */

#[cfg(target_arch = "arm")]
fn main() -> Result<(), Box<dyn Error>> {
    pi::main()
}

#[cfg(target_arch = "arm")]
mod pi {
    use crate::coffeepot::{Coffeepot, PotState};
    use crate::debounce;
    use rppal::gpio::{Gpio, Level};
    use rppal::pwm::{Channel, Polarity, Pwm};
    use std::error::Error;
    use std::io::{stdout, Write};
    use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
    use std::thread;
    use std::time::Duration;

    // Gpio uses BCM pin numbering
    const GPIO_READY_PIN: u8 = 17;
    const GPIO_POWER_PIN: u8 = 20;
    const PWM_READY_LED_PIN: Channel = Channel::Pwm1;
    const PWM_POWER_LED_PIN: Channel = Channel::Pwm0;

    fn ready_pwm_init(pwm: Channel) -> (Sender<Action>, thread::JoinHandle<()>) {
        let (tx, rx) = channel();
        (tx, thread::spawn(move || cycle_pwm(pwm, rx)))
    }

    fn pwm_cycle_wait(led: &Pwm, rx: &Receiver<Action>) -> bool {
        match rx.recv() {
            Ok(Action::Start) => false,
            Ok(Action::Stop(brightness)) => {
                led.set_duty_cycle(brightness).unwrap();
                pwm_cycle_wait(led, rx)
            },
            Ok(Action::Exit) => true,
            Err(_) => true,
        }
    }

    fn cycle_pwm(pwm: Channel, rx: Receiver<Action>) {
        let led = Pwm::with_period(
            pwm,
            Duration::from_millis(5),
            Duration::from_millis(2),
            Polarity::Normal,
            true,
        )
        .expect("Could not setup pwm pin");
        let mut exit = false;
        while !exit {
            for x in (0..100).chain((0..100).rev()) {
                led.set_duty_cycle((x as f64) / 100.0).unwrap();
                exit = match rx.recv_timeout(Duration::from_millis(10)) {
                    Ok(Action::Stop(brightness)) => {
                        led.set_duty_cycle(brightness).unwrap();
                        pwm_cycle_wait(&led, &rx)
                    }
                    Err(RecvTimeoutError::Timeout) => false,
                    Ok(Action::Start) => false,
                    Err(_) => true,
                    Ok(Action::Exit) => true,
                }
            }
        }
    }

    enum Action {
        Start,
        Stop(f64),
        Exit,
    }

    /** This is the actual main function running in production on rpi hardware */
    pub fn main() -> Result<(), Box<dyn Error>> {
        println!("Hello, world!");
        let mut ready_input = Gpio::new()?.get(GPIO_READY_PIN)?.into_input_pulldown();
        let mut power_input = Gpio::new()?.get(GPIO_POWER_PIN)?.into_input_pulldown();

        let (tx, pwm_thread) = ready_pwm_init(PWM_READY_LED_PIN);
        let power_led = Pwm::with_period(
            PWM_POWER_LED_PIN,
            Duration::from_millis(5),
            Duration::from_millis(2),
            Polarity::Normal,
            true,
        )
        .expect("Could not setup pwm power pin");

        let coffeepot = Coffeepot::new({
            let tx = tx.clone();
            move |new_state| {
                println!("state changed to {:?}", new_state);
                if new_state == PotState::Waiting {
                    tx.send(Action::Start).unwrap();
                } else if new_state == PotState::Idle {
                    tx.send(Action::Stop(0.0)).unwrap();
                } else {
                    tx.send(Action::Stop(0.9)).unwrap();
                }

                let power_brightness = match new_state {
                    PotState::Idle | PotState::Active => 0.1,
                    _ => 0.0,
                };
                power_led.set_duty_cycle(power_brightness).unwrap();
            }
        });
        let update_ready = debounce::closure(Level::Low, {
            let coffeepot = coffeepot.clone();
            move |level| {
                if level == Level::High {
                    coffeepot.toggle_ready();
                }
            }
        });
        let update_power = debounce::closure(Level::Low, {
            let coffeepot = coffeepot.clone();
            move |level| {
                if level == Level::High {
                    coffeepot.toggle_active();
                }
            }
        });
        ready_input.set_async_interrupt(rppal::gpio::Trigger::Both, update_ready)?;
        power_input.set_async_interrupt(rppal::gpio::Trigger::Both, update_power)?;
        //#[cfg(debug)]
        super::demo(coffeepot);
        #[cfg(not(debug))]
        loop {}
        tx.send(Action::Exit)?;
        println!("waiting for pwm thread to shut down");
        pwm_thread.join().unwrap();
        println!("exiting");
        Ok(())
    }

}
