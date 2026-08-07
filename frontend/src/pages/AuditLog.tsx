import { useEffect, useState } from 'react'
import { api, ApiError, type AuditLogEntry } from '../api'

/**
 * Kurzfassung von `before`/`after` für die Übersichtstabelle — z. B. die
 * Klartext-Meldung eines Benachrichtigungs-Checks (`{"message": "..."}`,
 * siehe havenmail-cli notify-check) oder ein knapper JSON-Auszug für
 * andere Aktionen. Kein Ersatz für eine vollständige Detailansicht, nur
 * damit die Tabelle nicht nur den rohen Aktionsnamen zeigt.
 */
function summarizeDetails(value: unknown): string {
  if (value == null) return '—'
  if (typeof value === 'object' && value !== null && 'message' in value) {
    const message = (value as { message?: unknown }).message
    if (typeof message === 'string') return message
  }
  const json = JSON.stringify(value)
  return json.length > 80 ? `${json.slice(0, 80)}…` : json
}

const PAGE_SIZE = 50

/**
 * Zeigt die Audit-Log-Hash-Chain (havenmail_core::audit), seitenweise per
 * Cursor ("mehr laden" statt Seitenzahlen — seq ist strikt monoton, ein
 * Cursor bleibt stabil auch wenn zwischenzeitlich neue Einträge dazu-
 * kommen, siehe routes/audit.rs). super_admin sieht alle Domains,
 * domain_admin nur die eigene (serverseitig erzwungen).
 */
export default function AuditLog() {
  const [entries, setEntries] = useState<AuditLogEntry[]>([])
  const [actions, setActions] = useState<string[]>([])
  const [actionFilter, setActionFilter] = useState('')
  const [since, setSince] = useState('')
  const [until, setUntil] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [forbidden, setForbidden] = useState(false)
  const [loading, setLoading] = useState(true)
  const [loadingMore, setLoadingMore] = useState(false)
  const [hasMore, setHasMore] = useState(true)

  function currentFilter(beforeSeq?: number) {
    return {
      action: actionFilter || undefined,
      since: since ? new Date(since).toISOString() : undefined,
      until: until ? new Date(until).toISOString() : undefined,
      beforeSeq,
      limit: PAGE_SIZE,
    }
  }

  function reload() {
    setLoading(true)
    setError(null)
    api.auditLog
      .list(currentFilter())
      .then((page) => {
        setEntries(page)
        setHasMore(page.length === PAGE_SIZE)
      })
      .catch((err: unknown) => {
        if (err instanceof ApiError && err.status === 403) {
          setForbidden(true)
        } else {
          setError('Audit-Log konnte nicht geladen werden.')
        }
      })
      .finally(() => setLoading(false))
  }

  async function loadMore() {
    if (entries.length === 0) return
    setLoadingMore(true)
    try {
      const lastSeq = entries[entries.length - 1].seq
      const page = await api.auditLog.list(currentFilter(lastSeq))
      setEntries((prev) => [...prev, ...page])
      setHasMore(page.length === PAGE_SIZE)
    } catch (err) {
      setError(err instanceof ApiError ? err.message : 'Weitere Einträge konnten nicht geladen werden.')
    } finally {
      setLoadingMore(false)
    }
  }

  useEffect(() => {
    api.auditLog.actions().then(setActions).catch(() => {})
  }, [])
  // eslint-disable-next-line react-hooks/exhaustive-deps
  useEffect(reload, [actionFilter, since, until])

  return (
    <div>
      <h1>Audit-Log</h1>
      {forbidden && (
        <div className="card">
          <p className="badge badge-not_ready">Keine Berechtigung, das Audit-Log einzusehen.</p>
        </div>
      )}
      {!forbidden && (
        <div className="card">
          <div className="inline-form" style={{ marginBottom: '1rem' }}>
            <label>
              Aktion
              <select value={actionFilter} onChange={(e) => setActionFilter(e.target.value)}>
                <option value="">Alle</option>
                {actions.map((a) => (
                  <option key={a} value={a}>
                    {a}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Von
              <input type="date" value={since} onChange={(e) => setSince(e.target.value)} />
            </label>
            <label>
              Bis
              <input type="date" value={until} onChange={(e) => setUntil(e.target.value)} />
            </label>
          </div>

          {error && <p className="badge badge-not_ready">{error}</p>}
          {loading && <p className="muted">Lade…</p>}
          {!loading && entries.length === 0 && !error && (
            <p className="muted">Keine Einträge für diese Filter.</p>
          )}
          {entries.length > 0 && (
            <div className="table-wrap">
              <table className="data-table">
                <thead>
                  <tr>
                    <th>Zeitpunkt</th>
                    <th>Aktion</th>
                    <th>Ziel</th>
                    <th>Details</th>
                    <th>IP</th>
                  </tr>
                </thead>
                <tbody>
                  {entries.map((entry) => (
                    <tr key={entry.id}>
                      <td>{new Date(entry.created_at).toLocaleString('de-DE')}</td>
                      <td>{entry.action}</td>
                      <td>{entry.target}</td>
                      <td className="muted">{summarizeDetails(entry.after ?? entry.before)}</td>
                      <td>{entry.ip ?? '—'}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
          {hasMore && entries.length > 0 && (
            <button onClick={loadMore} disabled={loadingMore} style={{ marginTop: '0.75rem' }}>
              {loadingMore ? 'Lädt…' : 'Weitere laden'}
            </button>
          )}
        </div>
      )}
    </div>
  )
}
