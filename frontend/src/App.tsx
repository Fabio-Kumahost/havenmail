/**
 * Havenmail Admin-Oberfläche.
 *
 * STATUS (M0): Skeleton. Zeigt den API-Health-Status an. Die vollständige
 * Admin-Oberfläche (Dashboard, Domains, Benutzer, Aliase, Queues, DNS-
 * Assistent, TLS, Audit-Log, Backups, ...) folgt in Meilenstein M4, siehe
 * docs/architecture.md im Repo-Root.
 */
import { useEffect, useState } from 'react'
import './App.css'

type HealthStatus = 'checking' | 'ok' | 'unreachable'

function App() {
  const [status, setStatus] = useState<HealthStatus>('checking')

  useEffect(() => {
    const apiBase = import.meta.env.VITE_HAVENMAIL_API_URL ?? 'http://127.0.0.1:8080'
    fetch(`${apiBase}/healthz`)
      .then((res) => (res.ok ? setStatus('ok') : setStatus('unreachable')))
      .catch(() => setStatus('unreachable'))
  }, [])

  return (
    <main>
      <h1>Havenmail</h1>
      <p>Eigenständige Mailserver-Plattform — Admin-Oberfläche (Skeleton, M0)</p>
      <p>
        Control-Plane API: <strong>{status}</strong>
      </p>
    </main>
  )
}

export default App
