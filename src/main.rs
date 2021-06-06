mod coffeepot;
mod debounce;
#[macro_use] extern crate log;
extern crate simplelog;
use chrono::prelude::*;
use coffeepot::Coffeepot;
use rumqtt::{MqttClient, MqttOptions, Notification, QoS, Receiver, ReconnectOptions};
use rumqtt::mqttoptions::SecurityOptions;
use std::env;
use std::error::Error;
use std::io::stdin;

pub fn init_mqtt(url: &str, port: u16) -> (MqttClient, Receiver<Notification>) {
    let creds = env::var_os("COFFEEPOT_USER")
        .and_then(|user_os| user_os.into_string().ok())
        .and_then(|user|
                  env::var_os("COFFEEPOT_PASS")
                  .and_then(|pass_os| pass_os.into_string().ok())
                  .map(|pass|
                       SecurityOptions::UsernamePassword(user,pass)
                      )
    ).unwrap_or(SecurityOptions::None);

    let reconnection_options = ReconnectOptions::Always(10);
    let mqtt_options = MqttOptions::new("coffeepot", url, port)
        .set_keep_alive(10)
        .set_inflight(3)
        .set_request_channel_capacity(10)
        .set_reconnect_opts(reconnection_options)
        .set_security_opts(creds)
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
                    warn!("payload empty!");
                    continue;
                }
                debug!("payload received {:?}", packet.payload);
                match packet.payload[0] as char {
                    'a' => coffeepot.activate(chrono::Duration::seconds(2)),
                    'i' => coffeepot.inactivate(),
                    'd' => {
                        const MINUTES: i32 = 60;
                        if packet.payload.len() == 1 {
                            coffeepot.activate_delayed(
                                chrono::Duration::minutes(45),
                                Local::now() + FixedOffset::east(5 * MINUTES),
                                );
                                continue;
                        };
                        let delay_str: Result<&str, Box<dyn Error>> = std::str::from_utf8(&packet.payload[1..])
                            .map_err(|e| e.into());
                        match delay_str.and_then(|s| s.parse::<i64>().map_err(|e| e.into())) {
                            Ok(delay) => {
                                debug!("delay {}", delay);
                                coffeepot.activate_delayed(
                                    chrono::Duration::minutes(90),
                                    Local::now() + chrono::Duration::minutes(delay),
                                    )
                            },
                            Err(e) => error!("vec -> string -> int failed: {}", e)
                        }
                    },
                    other => warn!("unexpected input: {}", other),
                }
                info!("state: {:?}", coffeepot.current_state());
            }
            _ => (),
        }
    }
}

/** Allows actions to be injected from the terminal for testing purposes */
#[allow(dead_code)]
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
            other => warn!("unexpected input: {}", other),
        }
        info!("state: {:?}", coffeepot.current_state());
    }
    Ok(())
}

/** This main function can be compiled and run on x86 machines to test the
 * state machine without any connected hardware */
#[allow(dead_code)]
#[cfg(not(target_arch = "arm"))]
fn main() -> Result<(), Box<dyn Error>> {
    use coffeepot::PotState;
    use simplelog::*;
    use std::thread;
    println!("Hello, world!");
    TermLogger::init(log::LevelFilter::Debug, Config::default(), TerminalMode::Mixed, ColorChoice::Auto).unwrap();
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
    use rumqtt::QoS;
    use std::error::Error;
    #[cfg(not(debug_assertions))]
    use std::sync::Arc;
    #[cfg(not(debug_assertions))]
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
    use std::thread;
    use std::time::Duration;
    use simplelog::*;
    use simplelog::{Level as LogLevel};

    // Gpio uses BCM pin numbering
    const GPIO_READY_BUTTON_PIN: u8 = 17;
    const GPIO_POWER_BUTTON_PIN: u8 = 22;
    const GPIO_RELAY_CTRL_PIN: u8 = 27;
    const PWM_READY_LED_PIN: Channel = Channel::Pwm1;
    const PWM_POWER_LED_PIN: Channel = Channel::Pwm0;

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
                        led.set_duty_cycle(brightness)
                            .expect("could not set ready led duty cycle");
                        pwm_cycle_wait(&led, &rx)
                    }
                    Err(RecvTimeoutError::Timeout) => false,
                    Ok(Action::Start) => false,
                    Err(e) => {
                        println!("error while receiving in pwm thread: {}", e);
                        true
                    },
                    Ok(Action::Exit) => true,
                };
                if exit {
                    break;
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
        let log_config = ConfigBuilder::new()
            .set_level_color(LogLevel::Error, Some(Color::Rgb(191, 0, 0)))
            .set_level_color(LogLevel::Warn, Some(Color::Rgb(255, 127, 0)))
            .set_level_color(LogLevel::Info, Some(Color::Rgb(192, 192, 0)))
            .set_level_color(LogLevel::Debug, Some(Color::Rgb(63, 127, 0)))
            .set_level_color(LogLevel::Trace, Some(Color::Rgb(127, 127, 255)))
            .add_filter_allow_str("coffeepot")
            .set_thread_mode(ThreadLogMode::Both)
            .set_location_level(LevelFilter::Debug)
            .set_thread_padding(ThreadPadding::Left(12))
            .build();
        #[cfg(debug_assertions)]
        TermLogger::init(log::LevelFilter::Debug, log_config, TerminalMode::Mixed, ColorChoice::Auto)?;
        #[cfg(not(debug_assertions))]
        {
            let log_file = std::fs::File::create("/var/log/coffeepot.log").expect("could not open log file");
            WriteLogger::init(LevelFilter::Debug, log_config, log_file)?;
        }
        info!("booting up coffeepot");
        let mut ready_input = Gpio::new()?.get(GPIO_READY_BUTTON_PIN)?.into_input_pulldown();
        let mut power_input = Gpio::new()?.get(GPIO_POWER_BUTTON_PIN)?.into_input_pulldown();
        let mut relay_output = Gpio::new()?.get(GPIO_RELAY_CTRL_PIN)?.into_output();

        let (pwm_tx, pwm_rx) = channel();
        // have to clone before first recv_timeout to avoid panic bug
        let pwm_tx2 = pwm_tx.clone();
        let pwm_thread = thread::spawn(move || cycle_pwm(PWM_READY_LED_PIN, pwm_rx));
        let power_led = Pwm::with_period(
            PWM_POWER_LED_PIN,
            Duration::from_millis(5),
            Duration::from_millis(2),
            Polarity::Normal,
            true,
        )
        .expect("Could not setup pwm power pin");
        info!("initialised pins");
        let (mqtt_tx, mqtt_rx) = crate::init_mqtt("bosch.hnrklssn.se", 1883);
        info!("connected to mqtt");

        let coffeepot = Coffeepot::new({
            let pwm_tx = pwm_tx2;
            let mut mqtt_tx = mqtt_tx;
            move |new_state| {
                info!("state changed to {:?}", new_state);
                    let result = if new_state == PotState::Waiting {
                        pwm_tx.send(Action::Start)
                    } else if new_state == PotState::Idle {
                        pwm_tx.send(Action::Stop(0.0))
                    } else {
                        pwm_tx.send(Action::Stop(0.9))
                    };
                    match result {
                        Err(e) => {
                            error!("error sending to pwm: {}", e);
                            std::panic!("error sending to pwm: {}", e);
                        },
                        _ => (),
                    }

                let power_brightness = match new_state {
                    PotState::Idle | PotState::Active => 0.1,
                    _ => 0.0,
                };
                power_led.set_duty_cycle(power_brightness)
                    .map_err(|_| error!("could not set duty cycle of power led"))
                    .expect("could not set duty cycle of power led");

                let relay_level = match new_state {
                    PotState::Active => Level::High,
                    _ => Level::Low,
                };
                relay_output.write(relay_level);
                mqtt_tx.publish(
                    "coffeepot/state",
                    QoS::AtLeastOnce,
                    false,
                    vec![new_state as u8],
                    )
                    .map_err(|_| error!("mqtt publish failed"))
                    .expect("mqtt publish failed");
            }
        });
        let update_ready = debounce::closure(Level::Low, {
            let coffeepot = coffeepot.clone();
            move |level| {
                debug!("update ready state {:?}", level);
                if level == Level::High {
                    coffeepot.toggle_ready();
                }
            }
        });
        let update_power = debounce::closure(Level::Low, {
            let coffeepot = coffeepot.clone();
            move |level| {
                debug!("update power state {:?}", level);
                if level == Level::High {
                    coffeepot.toggle_active();
                }
            }
        });
        ready_input.set_async_interrupt(rppal::gpio::Trigger::Both, update_ready)?;
        power_input.set_async_interrupt(rppal::gpio::Trigger::Both, update_power)?;
        thread::spawn({
            let coffeepot = coffeepot.clone();
            move || crate::handle_notifications(coffeepot, mqtt_rx)
        });
        // make sure main thread dies if pwm thread fails
        pwm_tx.send(Action::Stop(0.0))
            .map_err(|_| error!("pwm thread crashed on startup"))
            .expect("pwm thread crashed on startup");

        #[cfg(debug_assertions)]
        super::demo(coffeepot).expect("demo failed");
        #[cfg(not(debug_assertions))]
        {
            let shutdown = Arc::new(AtomicBool::new(false));
            let shutdown2 = shutdown.clone();
            ctrlc::set_handler(move || {
                info!("received Ctrl+C");
                shutdown2.store(true, Ordering::Relaxed);
            })
            .map_err(|_| error!("Error setting Ctrl-C handler"))
            .expect("Error setting Ctrl-C handler");
            while !shutdown.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_millis(5));
            }
            info!("initiating shutdown");
        }
        pwm_tx.send(Action::Exit)?;
        info!("waiting for pwm thread to shut down");
        pwm_thread.join()
            .map_err(|_| error!("joining pwm thread failed"))
            .expect("joining pwm thread failed");
        info!("exiting");
        Ok(())
    }

}
