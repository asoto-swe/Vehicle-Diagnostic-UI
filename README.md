# Vehicle Diagnostic UI

A full-stack vehicle diagnostic prototype that combines a simulated ECU, a Rust diagnostic backend, and a React/TypeScript technician UI.

## What is included

- Simulated ECUs with realistic fault injection for battery, motor, and thermal systems
- Rust backend exposing REST and WebSocket endpoints for diagnostics
- React/TypeScript frontend for scanning faults, viewing live data, and clearing DTCs
- Documentation that explains the architecture and fault reasoning

## Quick start

### 1. Start the ECU simulator

```bash
cd ecu-sim
cargo run
```

The simulator listens on http://127.0.0.1:3031.

### 2. Start the backend API

```bash
cd diag-backend
cargo run
```

The API listens on http://127.0.0.1:3000.

### 3. Start the frontend

```bash
cd diag-ui
npm install
npm run dev
```

Open the Vite URL shown in the terminal.

## Architecture

- ECU simulator: Rust service that exposes UDS-like services such as session control, data readout, DTC scan, clear, security access, and routine control.
- Backend: Axum API that translates between the UI and the ECU simulator.
- Frontend: React UI for technician workflow.

## Notes

This is an MVP focused on clarity and portfolio value rather than full ISO-TP/CAN implementation. The design and fault logic are structured to be a strong foundation for later work with SocketCAN and real UDS stacks.
