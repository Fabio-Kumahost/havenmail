import { useEffect, useState } from 'react'
import { api, ApiError, type AuditLogEntry } from '../api'

/**
 * Zeigt die letzten Einträge der unveränderlichen Audit-Log-Hash-Chain
 * (havenmail_core::audit). super_admin sieht alle Domains, domain_admin
 * nur die eigene (serverseitig erzwungen, siehe routes/audit.rs) — die
 * Seite selbst filtert nicht zusätzlich, sie zeigt genau das, was die API
 * für die jeweilige Rolle zurückgibt.
 */
export default function AuditLog() {
  const [entries, setEntries] = useState<AuditLogEntry[] | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    api.auditLog
      .list()
      .then(setEntries)
      .catch((err: unknown) => {
        if (err instanceof ApiError && err.status === 403) {
          setError('Keine Berechtigung, das Audit-Log einzusehen.')
        } else {
          setError('Audit-Log konnte nicht geladen werden.')
        }
      })
  }, [])

  return (
    <div>
      <h1>Audit-Log</h1>
      <div className="card">
        {error && <p className="badge badge-not_ready">{error}</p>}
        {!error && !entries && <p className="muted">Lade…</p>}
        {entries && entries.length === 0 && <p className="muted">Noch keine Einträge.</p>}
        {entries && entries.length > 0 && (
          <table className="data-table">
            <thead>
              <tr>
                <th>Zeitpunkt</th>
                <th>Aktion</th>
                <th>Ziel</th>
                <th>IP</th>
              </tr>
            </thead>
            <tbody>
              {entries.map((entry) => (
                <tr key={entry.id}>
                  <td>{new Date(entry.created_at).toLocaleString('de-DE')}</td>
                  <td>{entry.action}</td>
                  <td>{entry.target}</td>
                  <td>{entry.ip ?? '—'}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
        <p className="muted">Zeigt die letzten 50 Einträge. Filterung/Pagination folgt.</p>
      </div>
    </div>
  )
}
