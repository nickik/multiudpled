## multiudpled

Is part of:

https://github.com/n0ctu/LEDs-get-crazy-payloads

It extends the functionality by adds 4 byte pixels that include Alpha shift on the last byte.

The application starts and binds multiple ports, on each port a framebuffer gets filled.

Then all framebuffers are composited and transformed to 3 bytes per pixel, and then sent to LETs-get-crazy-payloads.

The compositing happens by port number, lower ports are lower layers.


# Usage

Provide no params or all

cargo run <start-port : 9000> <LEDs-get-crazy-payloads Location : ledsgc.luxeria.ch:54321> <height : 24> <width: 48> <layers : 100>


This would open Ports 9000-9100.

