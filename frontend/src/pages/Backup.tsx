import { useEffect, useState } from 'react'
import { api, ApiError, type BackupStatus } from '../api'

function formatBytes(bytes: number): string {
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

export default function Backup() {
  const [status, setStatus] = useState<BackupStatus | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [forbidden, setForbidden] = useState(false)
  const [triggering, setTriggering] = useState(false)

  function reload() {
    api.backup
      .status()
      .then(setStatus)
      .catch((err: unknown) => {
        if (err instanceof ApiError && err.status === 403) {
          setForbidden(true)
        } else {
          setError('Status konnte nicht geladen werden.')
        }
      })
  }

  useEffect(() => {
    reload()
    const interval = setInterval(reload, 15_000)
    return () => clearInterval(interval)
  }, [])

  async function onTrigger() {
    setTriggering(true)
    setError(null)
    try {
      await api.backup.trigger()
      // Ein echtes Backup (pg_dump + tar über Maildaten) kann Minuten
      // dauern — der 15s-Poll oben holt den fertigen Stand ab, hier
      // nur kurz warten, damit der Button nicht sofort wieder klickbar wirkt.
      await new Promise((r) => setTimeout(r, 1500))
      reload()
    } catch (err) {
      setError(err instanceof ApiError ? err.message : 'Backup konnte nicht ausgelöst werden')
    } finally {
      setTriggering(false)
    }
  }

  return (
    <div>
      <h1>Backup</h1>
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
      {!forbidden && (
        <div className="card">
          <h2>Manuelles Backup</h2>
          <p className="muted">
            Automatisch läuft täglich um 03:00 Uhr ein Backup (Datenbank, Konfiguration inkl.
            Geheimnisse, Maildaten). Hier kann zusätzlich jederzeit manuell eines ausgelöst werden.
          </p>
          <button onClick={onTrigger} disabled={triggering}>
            {triggering ? 'Wird ausgelöst…' : 'Backup jetzt auslösen'}
          </button>
          {status?.last_run && (
            <p style={{ marginTop: '0.75rem' }}>
              Letzter Lauf:{' '}
              <span
                className={`badge badge-${status.last_run.status === 'success' ? 'ready' : 'not_ready'}`}
              >
                {status.last_run.status === 'success' ? 'erfolgreich' : 'fehlgeschlagen'}
              </span>{' '}
              <span className="muted">
                ({new Date(status.last_run.ran_at).toLocaleString('de-DE')})
              </span>
              {status.last_run.status === 'failed' && status.last_run.error && (
                <>
                  <br />
                  <span className="badge badge-not_ready">{status.last_run.error}</span>
                </>
              )}
            </p>
          )}
        </div>
      )}
      {!forbidden && status && (
        <div className="card">
          <h2>Verlauf</h2>
          {status.history.length === 0 ? (
            <p className="muted">Noch keine Backups vorhanden.</p>
          ) : (
            <table className="data-table">
              <thead>
                <tr>
                  <th>Datei</th>
                  <th>Größe</th>
                  <th>Erstellt</th>
                </tr>
              </thead>
              <tbody>
                {status.history.map((entry) => (
                  <tr key={entry.filename}>
                    <td>
                      <code>{entry.filename}</code>
                    </td>
                    <td>{formatBytes(entry.size_bytes)}</td>
                    <td>{new Date(entry.created_at).toLocaleString('de-DE')}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      )}
    </div>
  )
}
