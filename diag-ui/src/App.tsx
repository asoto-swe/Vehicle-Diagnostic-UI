import { useEffect, useRef, useState } from 'react';

type Dtc = {
  code: string;
  status: number;
  test_failed: boolean;
  pending: boolean;
  confirmed: boolean;
};

type EcuSnapshot = {
  dtcs: Dtc[];
  data: Record<string, number>;
};

type VehicleSnapshot = {
  bms: EcuSnapshot | null;
  motor: EcuSnapshot | null;
  thermal: EcuSnapshot | null;
};

type EcuName = 'bms' | 'motor' | 'thermal';

const API_URL = 'http://127.0.0.1:3000';
const WS_URL = 'ws://127.0.0.1:3000/ws';

const ECU_LABELS: Record<EcuName, string> = {
  bms: 'Battery Management',
  motor: 'Motor Controller',
  thermal: 'Thermal / Cooling',
};

const DATA_LABELS: Record<string, { label: string; unit?: string; format?: (v: number) => string }> = {
  battery_temp_c: { label: 'Battery temp', unit: '°C' },
  battery_soc: { label: 'State of charge', unit: '%' },
  rpm: { label: 'Motor RPM' },
  coolant_temp_c: { label: 'Coolant temp', unit: '°C' },
  pump_ok: { label: 'Coolant pump', format: (v) => (v ? 'OK' : 'FAILED') },
};

function App() {
  const [snapshot, setSnapshot] = useState<VehicleSnapshot | null>(null);
  const [status, setStatus] = useState('Connecting...');
  const [busy, setBusy] = useState<string | null>(null);
  const wsRef = useRef<WebSocket | null>(null);

  useEffect(() => {
    const ws = new WebSocket(WS_URL);
    wsRef.current = ws;
    ws.onopen = () => setStatus('Live telemetry streaming');
    ws.onmessage = (event) => setSnapshot(JSON.parse(event.data) as VehicleSnapshot);
    ws.onerror = () => setStatus('WebSocket error — is diag-backend running?');
    ws.onclose = () => setStatus('Disconnected');
    return () => ws.close();
  }, []);

  async function clearDtcs(ecu: EcuName) {
    setBusy(`${ecu}-clear`);
    try {
      await fetch(`${API_URL}/ecus/${ecu}/dtcs/clear`, { method: 'POST' });
    } finally {
      setBusy(null);
    }
  }

  async function unlockThermal() {
    setBusy('thermal-unlock');
    try {
      const res = await fetch(`${API_URL}/ecus/thermal/security/unlock`, { method: 'POST' });
      const body = await res.json();
      setStatus(body.unlocked ? 'Thermal ECU unlocked' : `Unlock failed: ${JSON.stringify(body)}`);
    } finally {
      setBusy(null);
    }
  }

  async function runPumpTest() {
    setBusy('thermal-routine');
    try {
      const res = await fetch(`${API_URL}/ecus/thermal/routine`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ action: 'start' }),
      });
      const body = await res.json();
      setStatus(body.result ? `Cooling pump test: ${body.result}` : `Routine failed: ${JSON.stringify(body)}`);
    } finally {
      setBusy(null);
    }
  }

  return (
    <div style={{ fontFamily: 'system-ui, sans-serif', maxWidth: 1080, margin: '0 auto', padding: 24, color: '#1a1a1a' }}>
      <h1 style={{ marginBottom: 4 }}>Vehicle Diagnostic UI</h1>
      <p style={{ color: '#555', marginTop: 0 }}>{status}</p>

      <div style={{ display: 'grid', gap: 16, gridTemplateColumns: 'repeat(auto-fit, minmax(300px, 1fr))' }}>
        <EcuPanel
          label={ECU_LABELS.bms}
          snapshot={snapshot?.bms ?? null}
          onClear={() => clearDtcs('bms')}
          busy={busy === 'bms-clear'}
        />
        <EcuPanel
          label={ECU_LABELS.motor}
          snapshot={snapshot?.motor ?? null}
          onClear={() => clearDtcs('motor')}
          busy={busy === 'motor-clear'}
        />
        <EcuPanel
          label={ECU_LABELS.thermal}
          snapshot={snapshot?.thermal ?? null}
          onClear={() => clearDtcs('thermal')}
          busy={busy === 'thermal-clear'}
        >
          <div style={{ display: 'flex', gap: 8, marginTop: 8 }}>
            <button onClick={unlockThermal} disabled={busy === 'thermal-unlock'}>
              Unlock (Security Access)
            </button>
            <button onClick={runPumpTest} disabled={busy === 'thermal-routine'}>
              Run Cooling Pump Test
            </button>
          </div>
        </EcuPanel>
      </div>
    </div>
  );
}

function EcuPanel({
  label,
  snapshot,
  onClear,
  busy,
  children,
}: {
  label: string;
  snapshot: EcuSnapshot | null;
  onClear: () => void;
  busy: boolean;
  children?: React.ReactNode;
}) {
  const dtcs = snapshot?.dtcs ?? [];
  const data = snapshot?.data ?? {};

  return (
    <div style={{ border: '1px solid #ddd', borderRadius: 8, padding: 16 }}>
      <h2 style={{ marginTop: 0, marginBottom: 8, fontSize: 18 }}>{label}</h2>

      <div style={{ marginBottom: 12 }}>
        {Object.entries(data).length === 0 && <p style={{ color: '#999', fontSize: 14 }}>Waiting for data…</p>}
        {Object.entries(data).map(([key, value]) => {
          const meta = DATA_LABELS[key] ?? { label: key };
          return (
            <div key={key} style={{ display: 'flex', justifyContent: 'space-between', fontSize: 14, padding: '2px 0' }}>
              <span style={{ color: '#555' }}>{meta.label}</span>
              <strong>{meta.format ? meta.format(value) : `${value}${meta.unit ?? ''}`}</strong>
            </div>
          );
        })}
      </div>

      <div style={{ marginBottom: 12 }}>
        {dtcs.length === 0 ? (
          <span style={{ fontSize: 13, color: '#2a8f4d' }}>● No faults</span>
        ) : (
          dtcs.map((dtc) => (
            <div key={dtc.code} style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '3px 0' }}>
              <span
                style={{
                  fontSize: 11,
                  fontWeight: 600,
                  padding: '2px 6px',
                  borderRadius: 4,
                  color: '#fff',
                  background: dtc.confirmed ? '#c0392b' : dtc.pending ? '#e0a800' : '#888',
                }}
              >
                {dtc.confirmed ? 'CONFIRMED' : dtc.pending ? 'PENDING' : 'STORED'}
              </span>
              <code style={{ fontSize: 13 }}>{dtc.code}</code>
            </div>
          ))
        )}
      </div>

      <button onClick={onClear} disabled={busy || dtcs.length === 0}>
        Clear DTCs
      </button>
      {children}
    </div>
  );
}

export default App;
