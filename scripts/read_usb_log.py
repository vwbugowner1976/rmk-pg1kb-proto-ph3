#!/usr/bin/env python3
"""Read RMK usb_log output from the keyboard's CDC ACM serial port.

Usage on Windows:
    py -m pip install pyserial
    py scripts\read_usb_log.py
    py scripts\read_usb_log.py COM7

When no COM port is supplied the script lists all available serial ports.
"""

from __future__ import annotations

import sys
import time

try:
    import serial
    from serial.tools import list_ports
except ImportError:
    print("pyserial is required: py -m pip install pyserial", file=sys.stderr)
    raise SystemExit(2)


def show_ports() -> None:
    ports = list(list_ports.comports())
    if not ports:
        print("No serial ports found.")
        return

    print("Available serial ports:")
    for port in ports:
        details = port.description or ""
        hwid = port.hwid or ""
        print(f"  {port.device:8}  {details}  {hwid}")


def main() -> int:
    if len(sys.argv) != 2:
        show_ports()
        print()
        print("Run again with the RMK logging COM port, for example:")
        print(r"  py scripts\read_usb_log.py COM7")
        return 0 if len(sys.argv) == 1 else 2

    port_name = sys.argv[1]
    print(f"Opening {port_name}. Ctrl+C to stop.")
    print("RMK boot-stage logs may already be gone; PAW3222 v2 prints a diagnostic heartbeat every second.")

    try:
        with serial.Serial(port_name, 115200, timeout=0.25) as port:
            while True:
                data = port.readline()
                if data:
                    print(data.decode("utf-8", errors="replace"), end="")
                else:
                    time.sleep(0.01)
    except KeyboardInterrupt:
        print("\nStopped.")
        return 0
    except serial.SerialException as exc:
        print(f"Serial error: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
