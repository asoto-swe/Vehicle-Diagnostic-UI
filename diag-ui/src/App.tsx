import { useEffect, useState } from 'react';

type Dtc = {
  code: string;
  status: string;
  subsystem: string;
};

type VehicleState = {
  battery_soc: number;
  battery_temp_c: number;
  coolant_temp_c: number;
  motor_rpm: number;
  coolant_pump_ok: boolean;
  session: string;
  dtcs: Dtc[];
};

const API_URL = 'http://127.0.0.1:3000';

function App() {
  const [vehicle, setVehicle] = useState<VehicleState | null>(null);
  const [message, setMessage] = useState('Connecting...');

  useEffect(() => {
    const loadState = async () => {
      try {
        const response = await fetch(`${API_URL}/diagnostics`);
        const data = (await response.json()) as VehicleState;
        setVehicle(data);
        setMessage('Vehicle connected');
      } catch {
        setMessage('Backend not reachable yet');
      }
    };

    loadState();
    const ws = new WebSocket('ws://127.0.0.1:3000/ws');
    ws.onmessage = (event) => {
      const data = JSON.parse(event.data) as VehicleState;
      setVehicle(data);
      setMessage('Live telemetry streaming');
    };
    ws.onerror = () => setMessage('WebSocket unavailable');
    return () => ws.close();
  }, []);

  const scanFaults = async () => {
    const response = await fetch(`${API_URL}/diagnostics/scan`, { method: 'POST' });
    const data = await response.json();
    setMessage(`Scan response: ${data.status}`);
  };

  const clearFaults = async () => {
    const response = await fetch(`${API_URL}/diagnostics/clear`, { method: 'POST' });
    const data = await response.json();
    setMessage(`Clear result: ${data.status}`);
  };

  return (
    <div style={{ fontFamily: 'Arial, sans-serif', maxWidth: 960, margin: '0 auto', padding: 24 }}>
      <h1>Vehicle Diagnostic UI</h1>
      <p>{message}</p>
      <div style={{ display: 'grid', gap: 16, gridTemplateColumns: 'repeat(auto-fit, minmax(220px, 1fr))' }}>
        <div style={{ border: '1px solid #ccc', padding: 16, borderRadius: 8 }}>
          <h2>Battery</h2>
          <p>State of charge: {vehicle?.battery_soc.toFixed(1)}%</p>
          <p>Temperature: {vehicle?.battery_temp_c.toFixed(1)}°C</p>
        </div>
        <div style={{ border: '1px solid #ccc', padding: 16, borderRadius: 8 }}>
          <h2>Thermal</h2>
          <p>Coolant temperature: {vehicle?.coolant_temp_c.toFixed(1)}°C</p>
          <p>Pump healthy: {vehicle?.coolant_pump_ok ? 'Yes' : 'No'}</p>
        </div>
        <div style={{ border: '1px solid #ccc', padding: 16, borderRadius: 8 }}>
          <h2>Powertrain</h2>
          <p>Motor RPM: {vehicle?.motor_rpm}</p>
          <p>Session: {vehicle?.session}</p>
        </div>
      </div>
      <div style={{ marginTop: 24, display: 'flex', gap: 12 }}>
        <button onClick={scanFaults}>Scan faults</button>
        <button onClick={clearFaults}>Clear faults</button>
      </div>
      <div style={{ marginTop: 24 }}>
        <h2>Active DTCs</h2>
        {vehicle?.dtcs.length ? vehicle.dtcs.map((dtc) => (
          <div key={dtc.code} style={{ borderBottom: '1px solid #eee', padding: '8px 0' }}>
            <strong>{dtc.code}</strong> — {dtc.status} ({dtc.subsystem})
          </div>
        )) : <p>No DTCs reported.</p>}
      </div>
    </div>
  );
}

export default App;
