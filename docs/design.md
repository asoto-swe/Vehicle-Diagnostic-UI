# Design Notes

## Goals

The project is designed to feel like a realistic diagnostic tooling stack rather than a toy dashboard. The implementation follows a layered model:

1. ECU simulation layer
2. Diagnostic backend layer
3. Technician UI layer

## Why this shape

- The ECU simulator models the behavior of a serviceable vehicle network without needing real hardware.
- The backend acts as the diagnostic gateway and exposes a clean API for the UI.
- The frontend is written in React/TypeScript so the technician workflow is easy to demonstrate and extend.

## Protocol choices

The simulator uses UDS-like service names to mirror real diagnostic concepts:

- `0x10` Diagnostic Session Control
- `0x22` Read Data By Identifier
- `0x19` Read DTC Information
- `0x14` Clear Diagnostic Information
- `0x27` Security Access
- `0x31` Routine Control

The implementation is intentionally simplified compared with the full ISO-TP/UDS stack, but the service naming and behavior are chosen to make the leap to a real implementation straightforward.

## Fault model

The simulated faults are meant to feel diagnostic rather than arbitrary:

- Battery temperature rising beyond threshold causes a battery-management DTC.
- Motor load and speed interactions create intermittent conditions.
- A cooling pump failure cascades into a secondary battery temperature issue.
- A plausibility fault is modeled as a stuck sensor that reports an in-range but suspicious value.

## Next steps

The natural next step is to replace the transport layer with real SocketCAN and a protocol stack such as ISO-TP plus a full UDS client/server implementation.
