import { useEffect, useState } from 'react'
import { api, ApiError, type SystemStatus } from '../api'

/**
 * Dienststatus der orchestrierten Mail-Engines (M5) — nur für super_admin
 * sichtbar (die API lehnt die Route für andere Rollen mit 403 ab). Zeigt,
 * ob Postfix/Dovecot/Rspamd/ClamAV/nginx/Fail2ban tatsächlich laufen, nicht
 * nur ob die Control-Plane-API selbst erreichbar ist.
 */
export default function System() {
  const [status, setStatus] = useState<SystemStatus | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    api.system
      .status()
      .then(setStatus)
      .catch((err: unknown) => {
        if (err instanceof ApiError && err.status === 403) {
          setError('Nur für Administratoren mit vollem Systemzugriff sichtbar.')
        } else {
          setError('Systemstatus konnte nicht geladen werden.')
        }
      })
  }, [])

  return (
    <div>
      <h1>System</h1>
      <div className="card">
        <h2>Datenbank</h2>
        {error && <p className="badge badge-not_ready">{error}</p>}
        {!error && !status && <p className="muted">Lade…</p>}
        {status && (
          <p>
            Verbindung:{' '}
            <span className={`badge badge-${status.database ? 'ready' : 'not_ready'}`}>
              {status.database ? 'erreichbar' : 'nicht erreichbar'}
            </span>
          </p>
        )}
      </div>
      {status && (
        <div className="card">
          <h2>Dienste</h2>
          <table className="data-table">
            <thead>
              <tr>
                <th>Dienst</th>
                <th>Status</th>
              </tr>
            </thead>
            <tbody>
              {status.services.map((service) => (
                <tr key={service.unit}>
                  <td>{service.unit}</td>
                  <td>
                    <span className={`badge badge-${service.active ? 'ready' : 'not_ready'}`}>
                      {service.detail}
                    </span>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {status && (
        <div className="card">
          <h2>TLS-Zertifikat</h2>
          {status.tls ? (
            <p>
              Läuft ab: {status.tls.expires_at}
              {status.tls.days_remaining !== null && (
                <>
                  {' — '}
                  <span
                    className={`badge badge-${status.tls.days_remaining > 14 ? 'ready' : 'not_ready'}`}
                  >
                    {status.tls.days_remaining >= 0
                      ? `noch ${status.tls.days_remaining} Tage`
                      : 'abgelaufen'}
                  </span>
                </>
              )}
            </p>
          ) : (
            <p className="muted">Kein Zertifikatsstatus verfügbar.</p>
          )}
        </div>
      )}
      {status && (
        <p className="muted">
          Backup-/Update-Status, Warteschlangen und Spam-/Virenereignisse sind noch nicht
          angebunden.
        </p>
      )}
    </div>
  )
}
