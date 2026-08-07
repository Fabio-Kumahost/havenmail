import { useEffect, useState } from 'react'
import { Link } from 'react-router-dom'
import { api, ApiError, type DomainOverviewEntry } from '../api'

function formatBytes(bytes: number | null | undefined): string {
  if (bytes == null) return '—'
  if (bytes < 1024) return `${bytes} B`
  const units = ['KB', 'MB', 'GB', 'TB']
  let value = bytes / 1024
  let unit = 0
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024
    unit += 1
  }
  return `${value.toFixed(1)} ${units[unit]}`
}

/**
 * Aggregierte Übersicht über alle Domains — für den Betrieb mehrerer
 * Kundendomains ("Reseller"-Blick): Speicher, Nutzeranzahl je Domain
 * nebeneinander statt einzeln auf jeder Domain-Detail-Seite nachsehen zu
 * müssen. Nur super_admin (die API lehnt für andere Rollen mit 403 ab —
 * eine domänenübergreifende Übersicht ist für domain_admin per Definition
 * kein sinnvoller Ausschnitt).
 */
export default function Overview() {
  const [entries, setEntries] = useState<DomainOverviewEntry[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [forbidden, setForbidden] = useState(false)

  useEffect(() => {
    api.domains
      .overview()
      .then(setEntries)
      .catch((err: unknown) => {
        if (err instanceof ApiError && err.status === 403) {
          setForbidden(true)
        } else {
          setError('Übersicht konnte nicht geladen werden.')
        }
      })
  }, [])

  const totals = entries?.reduce(
    (acc, e) => ({
      users: acc.users + e.user_count,
      storage: acc.storage + (e.storage_bytes ?? 0),
    }),
    { users: 0, storage: 0 },
  )

  return (
    <div>
      <h1>Übersicht</h1>
      {forbidden && (
        <div className="card">
          <p className="badge badge-not_ready">Nur für Administratoren mit vollem Systemzugriff sichtbar.</p>
        </div>
      )}
      {!forbidden && error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      {!forbidden && !entries && !error && (
        <div className="card">
          <p className="muted">Lädt…</p>
        </div>
      )}
      {!forbidden && entries && (
        <>
          {totals && entries.length > 0 && (
            <div className="card">
              <p style={{ margin: 0 }}>
                <strong>{entries.length}</strong> Domains, <strong>{totals.users}</strong> Postfächer
                insgesamt, <strong>{formatBytes(totals.storage)}</strong> belegter Speicher.
              </p>
            </div>
          )}
          <div className="card">
            <div className="table-wrap">
              <table className="data-table">
                <thead>
                  <tr>
                    <th>Domain</th>
                    <th>Status</th>
                    <th>Postfächer</th>
                    <th>Speicher</th>
                    <th>Domain-Quota</th>
                  </tr>
                </thead>
                <tbody>
                  {entries.map((e) => (
                    <tr key={e.id}>
                      <td>
                        <Link to={`/domains/${e.id}`}>{e.name}</Link>
                      </td>
                      <td>
                        <span className={`badge badge-${e.is_active ? 'ready' : 'not_ready'}`}>
                          {e.is_active ? 'aktiv' : 'inaktiv'}
                        </span>
                      </td>
                      <td>{e.user_count}</td>
                      <td>{formatBytes(e.storage_bytes)}</td>
                      <td className="muted">{e.quota_bytes ? formatBytes(e.quota_bytes) : 'unbegrenzt'}</td>
                    </tr>
                  ))}
                  {entries.length === 0 && (
                    <tr>
                      <td colSpan={5} className="muted">
                        Keine Domains vorhanden.
                      </td>
                    </tr>
                  )}
                </tbody>
              </table>
            </div>
          </div>
        </>
      )}
    </div>
  )
}
