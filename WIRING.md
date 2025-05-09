# Component wiring for the payload
This document specifies the pins which are used for various components on the
Raspberry Pi in the AROWSS (Automatic Remote Onboard Wireless Streaming System)
payload. Pinout references can be found on [pinout.xyz](https://pinout.xyz/).

All pins are listed as GPIO pins, not board pins.

## UART Wiring
For UART connections, the TX pin of the device always connects to the RX pin on
the Raspberry Pi, and the RX pin of the device always connects to the TX pin on
the Raspberry Pi.

| UART | Component | Tx  | Rx  |
|------|:---------:|:---:|:---:|
|UART0 |           | 14  | 15  |
|~~UART1~~| DO NOT USE |~~14~~ |~~15~~ |
|UART2 | RFD-900x Radio | 0   | 1   |
|UART3 | ZED-F9P GPS | 4   | 5   |
|UART4 |           | 8   | 9   |
|UART5 |           | 12  | 13  |


## Other Wiring

| Component | Pin(s) |
|-----------|--------|
| I²C       | 2 (SDA), 3 (SCL) |
| Relay     | 21     |
