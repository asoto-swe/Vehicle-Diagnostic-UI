# Fault Catalogue

## Battery Management ECU

- DTC: `P0A80`
- Trigger: cell temperature exceeds the configured threshold.
- Meaning: the battery pack is overheating and requires inspection.

## Motor Controller ECU

- DTC: `C1234`
- Trigger: intermittent load conditions cause a fault to become active only during high load.
- Meaning: the issue is likely transient and should be checked under dynamic conditions.

## Thermal Control ECU

- DTC: `P0218`
- Trigger: cooling pump failure causes coolant temperature to climb.
- Meaning: root cause appears in the cooling system and may cause secondary battery temperature faults.

## Sensor Plausibility Fault

- DTC: `U0155`
- Trigger: the coolant temperature signal remains at `0°C` for an extended period even though the system is active.
- Meaning: the sensor may be stuck or disconnected.
