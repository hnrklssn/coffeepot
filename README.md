# Coffeepot
Firmware for Raspberry Pi based coffeemaker controller. Automation over MQTT.
Designed to start making coffee slightly *before* your alarm clock goes off,
to let you wake up to the bitter (read: sweet) smell of coffee in the morning.
How can a day starting out like that be anything but great?

Simplifies state management of the automation server by ignoring messages of
delayed start unless the user has "armed" the coffeemaker with water and coffee
grounds, as indicated by the user pressing the `ready` button on the controller.

# Hardware
I use a [Raspberry Pi Zero W](https://www.raspberrypi.org/products/raspberry-pi-zero-w/) because
it is an inexpensive microcontroller considering it has a built-in wifi module.
Even with the cost of an SD card it is still cheaper than some alternatives.
You can get somewhat cheaper microcontroller + wireless connection combinations, but I like the development
cycle of SSHing to the device and coding, compiling and debugging right on the device itself, so the extra cost is worth it.
This firmware is only compatible with Raspberry Pis though, so if you use another device you will have to write your own. :)

The electronics required is quite simple:
 - relay module that can handle the voltage of you mains electricity
 - 2 momentary switches
 - 2 state indicator LEDs (I suggest using switches with built in LEDs for a really sleek look)
 - 2 transistors to avoid killing GPIOs with LED current draw
 - 2 resistors limiting the current to the LEDs (many switches have this built in)
 - 5V power source
 - plugs to connect to mains power, and allow passthrough to a coffeemaker
 - (optionally) 2 capacitors for some hardware debouncing of the buttons. The built-in software debouncing should be enough though.
 
The electronics being simple is not the same as being a beginner project, however.
If you do not have a firm grasp of electrical theory and safety, do not mess with mains power!
I will not be providing instructions on how to construct any of the hardware interacting with
mains power, because I do not want to be responsible for your safety.
It should be a piece of cake to figure out how to wire it for the people who
have the knowledge required, anyways.

# Operation
There are 4 states:
 - `Idle`  
   The default state.
 - `Ready`  
   The user has indicated that the coffeemaker is prepped with coffee grounds and water.
 - `Waiting`  
   Delayed activation.
 - `Active`  
   The coffeemaker is receiving power.
 
Pressing the `power` button in any other state sets the state to `Active`.
Pressing it in `Active` sets the state to `Idle`.
Pressing the `ready` button in `Idle` sets the state to `Ready`, while pressing it in `Ready` sets it back to `Idle`.
`Waiting` can only be entered by receiving a delayed activation command over MQTT while in `Ready`.

# QTMBFAIAPAATT - Questions That Might Be Frequently Asked If Anyone Paid Any Attention To This
## Why don't you just get a coffeemaker with a built-in timer?
I want as much of my automated morning routine as possible to automatically adjust to the alarm time I set on my phone.
A built-in timer would require setting the wake-up time twice.

## Why don't you just get a wifi connected coffeemaker?
I have a nice coffeemaker that I'm very happy with otherwise.
Paying a lot of money for a smart coffee maker that potentially makes worse coffee doesn't appeal to me.

## Why don't you just get a smart outlet?
This solution would actually be cheaper than the required hardware for this project! But it's missing the `ready` button.
I don't want to break the glass pot if I forget to prep the machine, or if I'm on vacation.
This setup also lets me use a [NodeRED](https://nodered.org/) flow that monitors the current state to send a push notification
to remind me to prep the coffee (but only if I haven't already done so) when it's time for bed.
