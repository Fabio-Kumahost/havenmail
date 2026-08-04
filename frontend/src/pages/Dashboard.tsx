import { useEffect, useState } from 'react'
import { api } from '../api'

/**
 * STATUS (M4): zeigt bisher nur den API-/DB-Health-Status. Warteschlangen,
 * Zustellfehler, Spam-/Virenereignisse, Ressourcenauslastung usw. sind für
 * eine Folgephase geplant, sobald die Mail-Engines (Postfix/Dovecot/Rspamd)
 * tatsächlich installiert und deren Metriken/Log-Parsing angebunden sind.
 */
export default function Dashboard() {
  const [status, setStatus] = useState<'checking' | 'ready' | 'not_ready'>('checking')

  useEffect(() => {
    api.health
      .ready()
      .then((r) => setStatus(r.status === 'ready' ? 'ready' : 'not_ready'))
      .catch(() => setStatus('not_ready'))
  }, [])

  return (
    <div>
      <h1>Dashboard</h1>
      <div className="card">
        <h2>Serverzustand</h2>
        <p>
          Control-Plane-API: <span className={`badge badge-${status}`}>{status}</span>
        </p>
        <p className="muted">
          Warteschlangen, Zustellfehler, Spam-/Virenereignisse und Ressourcenauslastung werden
          angezeigt, sobald die Mail-Engines in einer Folgephase installiert sind.
        </p>
      </div>
    </div>
  )
}
