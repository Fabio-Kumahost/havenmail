import { useEffect, useMemo, useState } from 'react'
import {
  Area,
  AreaChart,
  CartesianGrid,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts'
import { api, ApiError, type MetricsPoint, type QueueEntry } from '../api'

type Range = '7d' | '30d'

function formatAxisTime(iso: string, range: Range) {
  const d = new Date(iso)
  return range === '7d'
    ? d.toLocaleDateString('de-DE', { weekday: 'short', hour: '2-digit' })
    : d.toLocaleDateString('de-DE', { day: '2-digit', month: '2-digit' })
}

function formatTooltipTime(iso: string) {
  return new Date(iso).toLocaleString('de-DE', {
    day: '2-digit',
    month: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  })
}

function sum(points: MetricsPoint[], key: keyof MetricsPoint): number {
  return points.reduce((total, p) => total + (typeof p[key] === 'number' ? (p[key] as number) : 0), 0)
}

/** Liest die aktuellen Design-Tokens aus dem DOM statt CSS-Variablen direkt
 * an Recharts durchzureichen — manche SVG-Attribute werden von Recharts vor
 * dem Rendern in Inline-Styles überführt, wo `var(--x)`-Auflösung nicht
 * immer zuverlässig greift. Läuft nur einmal pro Mount, Tokens ändern sich
 * nicht zur Laufzeit (nur Hell/Dunkel per OS-Umschaltung, dann Reload). */
function useChartColors() {
  return useMemo(() => {
    const styles = getComputedStyle(document.documentElement)
    const read = (name: string, fallback: string) => styles.getPropertyValue(name).trim() || fallback
    return {
      border: read('--border', '#e4e7ec'),
      muted: read('--muted', '#98a2b3'),
      surface: read('--surface', '#ffffff'),
      text: read('--text', '#101828'),
      accent: read('--accent', '#4f46e5'),
      ok: read('--ok', '#067647'),
      bad: read('--bad', '#b42318'),
    }
  }, [])
}

type ChartColors = ReturnType<typeof useChartColors>

function ChartTooltip({
  active,
  payload,
  label,
  colors,
  formatValue,
}: {
  active?: boolean
  payload?: { name: string; value: number; color: string }[]
  label?: string
  colors: ChartColors
  formatValue?: (value: number) => string
}) {
  if (!active || !payload || payload.length === 0) return null
  return (
    <div
      style={{
        background: colors.surface,
        border: `1px solid ${colors.border}`,
        borderRadius: '0.5rem',
        padding: '0.5rem 0.65rem',
        fontSize: '0.8rem',
        boxShadow: '0 4px 12px rgba(16,24,40,0.12)',
      }}
    >
      <div style={{ color: colors.muted, marginBottom: '0.25rem' }}>
        {label ? formatTooltipTime(label) : ''}
      </div>
      {payload.map((entry) => (
        <div key={entry.name} style={{ color: colors.text }}>
          <span style={{ color: entry.color }}>●</span> {entry.name}:{' '}
          {formatValue ? formatValue(entry.value) : entry.value}
        </div>
      ))}
    </div>
  )
}

function MailQueueSection() {
  const [entries, setEntries] = useState<QueueEntry[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busyId, setBusyId] = useState<string | null>(null)
  const [clearingAll, setClearingAll] = useState(false)

  function reload() {
    api.mailQueue
      .list()
      .then(setEntries)
      .catch((err: unknown) => {
        if (err instanceof ApiError && err.status === 403) {
          setError('Nur für Administratoren mit vollem Systemzugriff sichtbar.')
        } else {
          setError('Warteschlange konnte nicht geladen werden.')
        }
      })
  }
  useEffect(reload, [])

  async function onDeleteOne(queueId: string) {
    setBusyId(queueId)
    try {
      await api.mailQueue.deleteOne(queueId)
      reload()
    } catch (err) {
      setError(err instanceof ApiError ? err.message : 'Löschen fehlgeschlagen')
    } finally {
      setBusyId(null)
    }
  }

  async function onDeleteAll() {
    if (!entries || entries.length === 0) return
    if (!window.confirm(`Wirklich alle ${entries.length} Mails aus der Warteschlange löschen?`)) {
      return
    }
    setClearingAll(true)
    try {
      await api.mailQueue.deleteAll()
      reload()
    } catch (err) {
      setError(err instanceof ApiError ? err.message : 'Leeren fehlgeschlagen')
    } finally {
      setClearingAll(false)
    }
  }

  return (
    <div className="card">
      <h2>Mail-Warteschlange</h2>
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      {!error && !entries && <p className="muted">Lädt…</p>}
      {!error && entries && entries.length === 0 && <p className="muted">Warteschlange ist leer.</p>}
      {!error && entries && entries.length > 0 && (
        <>
          <div style={{ marginBottom: '0.9rem' }}>
            <button className="btn-danger" onClick={onDeleteAll} disabled={clearingAll}>
              {clearingAll ? 'Leere…' : `Alle ${entries.length} leeren`}
            </button>
          </div>
          <table className="data-table">
            <thead>
              <tr>
                <th>Absender</th>
                <th>Empfänger</th>
                <th>Grund</th>
                <th>Eingegangen</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {entries.map((entry) => (
                <tr key={entry.queue_id}>
                  <td>{entry.sender}</td>
                  <td>{entry.recipients.map((r) => r.address).join(', ')}</td>
                  <td className="muted">
                    {entry.recipients.find((r) => r.delay_reason)?.delay_reason ?? '—'}
                  </td>
                  <td>{new Date(entry.arrival_time).toLocaleString('de-DE')}</td>
                  <td>
                    <button
                      className="btn-danger"
                      onClick={() => onDeleteOne(entry.queue_id)}
                      disabled={busyId === entry.queue_id}
                    >
                      Löschen
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}
    </div>
  )
}

export default function Dashboard() {
  const [apiStatus, setApiStatus] = useState<'checking' | 'ready' | 'not_ready'>('checking')
  const [range, setRange] = useState<Range>('7d')
  const [points, setPoints] = useState<MetricsPoint[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const colors = useChartColors()

  useEffect(() => {
    api.health
      .ready()
      .then((r) => setApiStatus(r.status === 'ready' ? 'ready' : 'not_ready'))
      .catch(() => setApiStatus('not_ready'))
  }, [])

  useEffect(() => {
    setPoints(null)
    api.metrics
      .range(range)
      .then(setPoints)
      .catch(() => setError('Metriken konnten nicht geladen werden — nur für Administratoren mit vollem Systemzugriff sichtbar.'))
  }, [range])

  const latest = points && points.length > 0 ? points[points.length - 1] : null
  const spamTotal = points ? sum(points, 'spam_delta') : 0
  const hamTotal = points ? sum(points, 'ham_delta') : 0
  const virusTotal = points ? sum(points, 'virus_detected') : 0
  const sentTotal = points ? sum(points, 'mail_sent') : 0
  const receivedTotal = points ? sum(points, 'mail_received') : 0

  const chartData = useMemo(
    () =>
      (points ?? []).map((p) => ({
        ...p,
        time: formatAxisTime(p.captured_at, range),
      })),
    [points, range],
  )

  return (
    <div>
      <h1>Dashboard</h1>

      <div className="card">
        <p style={{ margin: 0 }}>
          Control-Plane-API: <span className={`badge badge-${apiStatus}`}>{apiStatus}</span>
        </p>
      </div>

      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}

      {!error && (
        <>
          <div className="stat-grid">
            <div className="card stat-card">
              <span className="stat-label">Gesendet</span>
              <span className="stat-value" style={{ color: colors.accent }}>
                {sentTotal}
              </span>
              <span className="muted">{range === '7d' ? 'letzte 7 Tage' : 'letzte 30 Tage'}</span>
            </div>
            <div className="card stat-card">
              <span className="stat-label">Empfangen</span>
              <span className="stat-value" style={{ color: colors.accent }}>
                {receivedTotal}
              </span>
              <span className="muted">{range === '7d' ? 'letzte 7 Tage' : 'letzte 30 Tage'}</span>
            </div>
            <div className="card stat-card">
              <span className="stat-label">Spam blockiert</span>
              <span className="stat-value" style={{ color: colors.bad }}>
                {spamTotal}
              </span>
              <span className="muted">{range === '7d' ? 'letzte 7 Tage' : 'letzte 30 Tage'}</span>
            </div>
            <div className="card stat-card">
              <span className="stat-label">Als Ham erkannt</span>
              <span className="stat-value" style={{ color: colors.ok }}>
                {hamTotal}
              </span>
              <span className="muted">{range === '7d' ? 'letzte 7 Tage' : 'letzte 30 Tage'}</span>
            </div>
            <div className="card stat-card">
              <span className="stat-label">Viren gefunden</span>
              <span className="stat-value">{virusTotal}</span>
              <span className="muted">{range === '7d' ? 'letzte 7 Tage' : 'letzte 30 Tage'}</span>
            </div>
            <div className="card stat-card">
              <span className="stat-label">Warteschlange</span>
              <span className="stat-value">{latest?.mail_queue_size ?? '—'}</span>
              <span className="muted">aktuell</span>
            </div>
            <div className="card stat-card">
              <span className="stat-label">Speicher belegt</span>
              <span className="stat-value">
                {latest?.disk_used_percent != null ? `${latest.disk_used_percent.toFixed(0)}%` : '—'}
              </span>
              <span className="muted">aktuell</span>
            </div>
          </div>

          <div className="range-toggle">
            <button
              className={range === '7d' ? 'active' : ''}
              onClick={() => setRange('7d')}
              type="button"
            >
              7 Tage
            </button>
            <button
              className={range === '30d' ? 'active' : ''}
              onClick={() => setRange('30d')}
              type="button"
            >
              30 Tage
            </button>
          </div>
          <p className="muted" style={{ marginTop: '0.5rem', fontSize: '0.82rem' }}>
            Werte zeigen den Verlauf seit Beginn der Aufzeichnung (alle 15 Minuten ein Messpunkt) —
            direkt nach der Aktivierung sind das nur wenige Minuten, nicht die volle Zeitspanne.
          </p>

          <div className="card">
            <h2>Gesendet vs. empfangen</h2>
            {points && points.length < 2 && (
              <p className="muted">
                Noch zu wenige Messpunkte für einen Verlauf — der Snapshot-Job läuft alle 15
                Minuten.
              </p>
            )}
            <div style={{ height: 260 }}>
              <ResponsiveContainer width="100%" height="100%">
                <LineChart data={chartData} margin={{ top: 8, right: 12, left: 0, bottom: 0 }}>
                  <CartesianGrid stroke={colors.border} vertical={false} />
                  <XAxis
                    dataKey="time"
                    stroke={colors.muted}
                    fontSize={12}
                    tickLine={false}
                    axisLine={{ stroke: colors.border }}
                  />
                  <YAxis
                    stroke={colors.muted}
                    fontSize={12}
                    tickLine={false}
                    axisLine={false}
                    width={32}
                  />
                  <Tooltip content={<ChartTooltip colors={colors} />} />
                  <Line
                    type="monotone"
                    dataKey="mail_sent"
                    name="Gesendet"
                    stroke={colors.accent}
                    strokeWidth={2}
                    dot={false}
                    connectNulls
                  />
                  <Line
                    type="monotone"
                    dataKey="mail_received"
                    name="Empfangen"
                    stroke={colors.accent}
                    strokeWidth={2}
                    strokeDasharray="6 4"
                    strokeOpacity={0.6}
                    dot={false}
                    connectNulls
                  />
                </LineChart>
              </ResponsiveContainer>
            </div>
            <div className="chart-legend">
              <span>
                <i style={{ background: colors.accent }} /> Gesendet
              </span>
              <span>
                <i style={{ background: colors.accent, opacity: 0.6, borderStyle: 'dashed' }} />{' '}
                Empfangen (gestrichelt)
              </span>
            </div>
          </div>

          <div className="card">
            <h2>Spam vs. als Ham erkannt</h2>
            <div style={{ height: 260 }}>
              <ResponsiveContainer width="100%" height="100%">
                <LineChart data={chartData} margin={{ top: 8, right: 12, left: 0, bottom: 0 }}>
                  <CartesianGrid stroke={colors.border} vertical={false} />
                  <XAxis
                    dataKey="time"
                    stroke={colors.muted}
                    fontSize={12}
                    tickLine={false}
                    axisLine={{ stroke: colors.border }}
                  />
                  <YAxis
                    stroke={colors.muted}
                    fontSize={12}
                    tickLine={false}
                    axisLine={false}
                    width={32}
                  />
                  <Tooltip content={<ChartTooltip colors={colors} />} />
                  <Line
                    type="monotone"
                    dataKey="ham_delta"
                    name="Ham"
                    stroke={colors.ok}
                    strokeWidth={2}
                    dot={false}
                    connectNulls
                  />
                  <Line
                    type="monotone"
                    dataKey="spam_delta"
                    name="Spam"
                    stroke={colors.bad}
                    strokeWidth={2}
                    strokeDasharray="6 4"
                    dot={false}
                    connectNulls
                  />
                </LineChart>
              </ResponsiveContainer>
            </div>
            <div className="chart-legend">
              <span>
                <i style={{ background: colors.ok }} /> Ham
              </span>
              <span>
                <i style={{ background: colors.bad, borderStyle: 'dashed' }} /> Spam (gestrichelt)
              </span>
            </div>
          </div>

          <div className="card">
            <h2>Mail-Warteschlange</h2>
            <div style={{ height: 180 }}>
              <ResponsiveContainer width="100%" height="100%">
                <AreaChart data={chartData} margin={{ top: 8, right: 12, left: 0, bottom: 0 }}>
                  <defs>
                    <linearGradient id="queueFill" x1="0" y1="0" x2="0" y2="1">
                      <stop offset="0%" stopColor={colors.accent} stopOpacity={0.35} />
                      <stop offset="100%" stopColor={colors.accent} stopOpacity={0} />
                    </linearGradient>
                  </defs>
                  <CartesianGrid stroke={colors.border} vertical={false} />
                  <XAxis
                    dataKey="time"
                    stroke={colors.muted}
                    fontSize={12}
                    tickLine={false}
                    axisLine={{ stroke: colors.border }}
                  />
                  <YAxis
                    stroke={colors.muted}
                    fontSize={12}
                    tickLine={false}
                    axisLine={false}
                    width={32}
                  />
                  <Tooltip content={<ChartTooltip colors={colors} />} />
                  <Area
                    type="monotone"
                    dataKey="mail_queue_size"
                    name="Warteschlange"
                    stroke={colors.accent}
                    strokeWidth={2}
                    fill="url(#queueFill)"
                    connectNulls
                  />
                </AreaChart>
              </ResponsiveContainer>
            </div>
          </div>

          <div className="card">
            <h2>Speicherauslastung</h2>
            <div style={{ height: 180 }}>
              <ResponsiveContainer width="100%" height="100%">
                <AreaChart data={chartData} margin={{ top: 8, right: 12, left: 0, bottom: 0 }}>
                  <defs>
                    <linearGradient id="diskFill" x1="0" y1="0" x2="0" y2="1">
                      <stop offset="0%" stopColor={colors.accent} stopOpacity={0.35} />
                      <stop offset="100%" stopColor={colors.accent} stopOpacity={0} />
                    </linearGradient>
                  </defs>
                  <CartesianGrid stroke={colors.border} vertical={false} />
                  <XAxis
                    dataKey="time"
                    stroke={colors.muted}
                    fontSize={12}
                    tickLine={false}
                    axisLine={{ stroke: colors.border }}
                  />
                  <YAxis
                    stroke={colors.muted}
                    fontSize={12}
                    tickLine={false}
                    axisLine={false}
                    width={40}
                    domain={[0, 100]}
                    unit="%"
                  />
                  <Tooltip content={<ChartTooltip colors={colors} formatValue={(v) => `${v}%`} />} />
                  <Area
                    type="monotone"
                    dataKey="disk_used_percent"
                    name="Belegt"
                    stroke={colors.accent}
                    strokeWidth={2}
                    fill="url(#diskFill)"
                    connectNulls
                  />
                </AreaChart>
              </ResponsiveContainer>
            </div>
          </div>

          <MailQueueSection />
        </>
      )}
    </div>
  )
}
