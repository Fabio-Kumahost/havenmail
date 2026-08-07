import { useEffect, useState } from 'react'
import { api, ApiError, type Fail2banStatus } from '../api'

const JAIL_LABELS: Record<string, string> = {
  sshd: 'SSH',
  'postfix-sasl': 'SMTP-Anmeldung',
  dovecot: 'IMAP-Anmeldung',
}

export default function Fail2Ban() {
  const [status, setStatus] = useState<Fail2banStatus | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [forbidden, setForbidden] = useState(false)
  const [busyIp, setBusyIp] = useState<string | null>(null)

  function reload() {
    api.fail2ban
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
    const interval = setInterval(reload, 30_000)
    return () => clearInterval(interval)
  }, [])

  async function onUnban(jail: string, ip: string) {
    setBusyIp(ip)
    try {
      await api.fail2ban.unban(jail, ip)
      await new Promise((r) => setTimeout(r, 600))
      reload()
    } catch (err) {
      setError(err instanceof ApiError ? err.message : 'Entsperren fehlgeschlagen')
    } finally {
      setBusyIp(null)
    }
  }

  const totalBanned = status?.jails.reduce((sum, j) => sum + j.banned.length, 0) ?? 0

  return (
    <div>
      <h1>Fail2Ban</h1>
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
      {!forbidden && !status && !error && (
        <div className="card">
          <p className="muted">Lädt…</p>
        </div>
      )}
      {!forbidden && status && (
        <>
          <div className="card">
            <p style={{ margin: 0 }}>
              Aktuell gesperrt: <span className="badge badge-ready">{totalBanned}</span>{' '}
              <span className="muted">
                (Stand: {new Date(status.updated_at).toLocaleTimeString('de-DE')}, aktualisiert
                alle 30s)
              </span>
            </p>
          </div>
          {status.jails.map((jail) => (
            <div className="card" key={jail.name}>
              <h2>{JAIL_LABELS[jail.name] ?? jail.name}</h2>
              {jail.banned.length === 0 ? (
                <p className="muted">Keine gesperrten IPs.</p>
              ) : (
                <div className="table-wrap">
                  <table className="data-table">
                    <thead>
                      <tr>
                        <th>IP-Adresse</th>
                        <th></th>
                      </tr>
                    </thead>
                    <tbody>
                      {jail.banned.map((ip) => (
                        <tr key={ip}>
                          <td>
                            <code>{ip}</code>
                          </td>
                          <td>
                            <button
                              onClick={() => onUnban(jail.name, ip)}
                              disabled={busyIp === ip}
                            >
                              {busyIp === ip ? 'Entsperre…' : 'Entsperren'}
                            </button>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </div>
          ))}
        </>
      )}
    </div>
  )
}
