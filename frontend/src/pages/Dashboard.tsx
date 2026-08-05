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
import { api, type MetricsPoint } from '../api'

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
              <span className="stat-label">Spam blockiert</span>
              <span className="stat-value" style={{ color: colors.bad }}>
                {spamTotal}
              </span>
              <span className="muted">{range === '7d' ? 'letzte 7 Tage' : 'letzte 30 Tage'}</span>
            </div>
            <div className="card stat-card">
              <span className="stat-label">Zugestellte Mail</span>
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

          <div className="card">
            <h2>Spam vs. zugestellte Mail</h2>
            {points && points.length < 2 && (
              <p className="muted">
                Noch zu wenige Messpunkte für einen Verlauf — der Snapshot-Job läuft alle 15
                Minuten, ein aussagekräftiger Verlauf braucht etwas Anlaufzeit.
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
                    dataKey="ham_delta"
                    name="Zugestellt"
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
                <i style={{ background: colors.ok }} /> Zugestellt
              </span>
              <span>
                <i style={{ background: colors.bad, borderStyle: 'dashed' }} /> Spam (gestrichelte
                Linie)
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
        </>
      )}
    </div>
  )
}
