import { useEffect, useState, type ChangeEvent, type FormEvent } from 'react'
import { useParams } from 'react-router-dom'
import {
  api,
  type Domain,
  type User,
  type Alias,
  type DnsRecommendations,
  type DnsCheckResult,
  type DkimKeyEntry,
  type ImportResponse,
  ApiError,
} from '../api'

/**
 * Aktuelle Mindestpasswortlänge (super_admin-konfigurierbar unter
 * /password-policy) — von den beiden Passwort-Formularen dieser Seite
 * geteilt (Passwort ändern in UserRow, Neuanlage in UserForm), damit sie
 * nicht synchron zwei fast identische Kopien pflegen. Fallback 12, solange
 * die Anfrage noch lädt oder fehlschlägt — Server bleibt ohnehin die
 * eigentliche Durchsetzungsstelle.
 */
function usePasswordMinLength(): number {
  const [minLength, setMinLength] = useState(12)
  useEffect(() => {
    api.securitySettings
      .passwordPolicy()
      .then((p) => setMinLength(p.min_password_length))
      .catch(() => {})
  }, [])
  return minLength
}

/**
 * Domain-eigenes Rate-Limit-Override (siehe routes/domains.rs) — leere
 * Felder bedeuten "kein Override, nutzt den globalen Wert" (null im
 * Backend), keine "0" oder ähnlicher Sentinel-Wert. Zeigt zusätzlich den
 * Hinweis, dass der globale Default auf der Spam-Schutz-Seite gilt, falls
 * kein Override gesetzt ist.
 */
function RatelimitOverrideSection({
  domain,
  onReload,
  onError,
}: {
  domain: Domain
  onReload: () => void
  onError: (msg: string) => void
}) {
  const [perHour, setPerHour] = useState(domain.ratelimit_per_hour_override?.toString() ?? '')
  const [burst, setBurst] = useState(domain.ratelimit_burst_override?.toString() ?? '')
  const [submitting, setSubmitting] = useState(false)
  const [success, setSuccess] = useState(false)

  useEffect(() => {
    setPerHour(domain.ratelimit_per_hour_override?.toString() ?? '')
    setBurst(domain.ratelimit_burst_override?.toString() ?? '')
  }, [domain.ratelimit_per_hour_override, domain.ratelimit_burst_override])

  async function onSubmit(e: FormEvent) {
    e.preventDefault()
    setSuccess(false)
    setSubmitting(true)
    try {
      await api.domains.updateRatelimitOverride(
        domain.id,
        perHour.trim() === '' ? null : Number(perHour),
        burst.trim() === '' ? null : Number(burst),
      )
      setSuccess(true)
      onReload()
    } catch (err) {
      onError(err instanceof ApiError ? err.message : 'Speichern fehlgeschlagen')
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <section className="card">
      <h2>Rate-Limiting</h2>
      <p className="muted">
        Überschreibt für diese Domain das globale Rate-Limit (siehe Spam-Schutz-Seite) — leer
        lassen, um wieder den globalen Wert zu nutzen.
      </p>
      <form className="inline-form" onSubmit={onSubmit}>
        <label>
          Mails pro Stunde
          <input
            type="number"
            min={1}
            placeholder="global"
            value={perHour}
            onChange={(e) => setPerHour(e.target.value)}
          />
        </label>
        <label>
          Burst
          <input
            type="number"
            min={1}
            placeholder="global"
            value={burst}
            onChange={(e) => setBurst(e.target.value)}
          />
        </label>
        {success && <p className="badge badge-ready">Gespeichert und angewendet</p>}
        <button type="submit" disabled={submitting}>
          {submitting ? 'Speichere…' : 'Speichern'}
        </button>
      </form>
    </section>
  )
}

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

export default function DomainDetail() {
  const { domainId } = useParams<{ domainId: string }>()
  const [domain, setDomain] = useState<Domain | null>(null)
  const [users, setUsers] = useState<User[]>([])
  const [aliases, setAliases] = useState<Alias[]>([])
  const [storageById, setStorageById] = useState<Record<string, number | null>>({})
  const [error, setError] = useState<string | null>(null)

  function reload() {
    if (!domainId) return
    api.domains.get(domainId).then(setDomain).catch(() => {})
    api.users.list(domainId).then(setUsers).catch(() => {})
    api.aliases.list(domainId).then(setAliases).catch(() => {})
    // Separater, langsamerer Aufruf (du -sb pro Mailbox) — soll die
    // schnelle Benutzerliste oben nicht blockieren, siehe
    // routes/users.rs::get_users_storage.
    api.users
      .storage(domainId)
      .then((rows) => setStorageById(Object.fromEntries(rows.map((r) => [r.id, r.bytes]))))
      .catch(() => {})
  }

  useEffect(reload, [domainId])

  if (!domainId) return null

  return (
    <div>
      <h1>{domain?.name ?? domainId}</h1>
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      <section className="card">
        <h2>Benutzer</h2>
        <UserForm domainId={domainId} onCreated={reload} onError={setError} />
        <div className="table-wrap">
          <table className="data-table">
            <thead>
              <tr>
                <th>Adresse</th>
                <th>Rolle</th>
                <th>Status</th>
                <th>Speicher</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {users.map((u) => (
                <UserRow
                  key={u.id}
                  user={u}
                  domainName={domain?.name}
                  storageBytes={storageById[u.id]}
                  onReload={reload}
                  onError={setError}
                />
              ))}
              {users.length === 0 && (
                <tr>
                  <td colSpan={5} className="muted">
                    Keine Benutzer.
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
        <CsvImportExport domainId={domainId} domainName={domain?.name} onImported={reload} onError={setError} />
      </section>

      <section className="card">
        <h2>Aliase</h2>
        <AliasForm domainId={domainId} onCreated={reload} onError={setError} />
        <div className="table-wrap">
        <table className="data-table">
          <thead>
            <tr>
              <th>Quelle</th>
              <th>Ziel(e)</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {aliases.map((a) => (
              <tr key={a.id}>
                <td>
                  {a.source}@{domain?.name}
                </td>
                <td>{a.destinations.join(', ')}</td>
                <td>
                  <button
                    className="btn-danger"
                    onClick={() =>
                      api.aliases
                        .delete(a.id)
                        .then(reload)
                        .catch((err) => setError(err instanceof ApiError ? err.message : 'Fehler'))
                    }
                  >
                    Löschen
                  </button>
                </td>
              </tr>
            ))}
            {aliases.length === 0 && (
              <tr>
                <td colSpan={3} className="muted">
                  Keine Aliase.
                </td>
              </tr>
            )}
          </tbody>
        </table>
        </div>
      </section>

      {domain && (
        <RatelimitOverrideSection domain={domain} onReload={reload} onError={setError} />
      )}

      <DnsSection domainId={domainId} onError={setError} />
    </div>
  )
}

function UserRow({
  user,
  domainName,
  storageBytes,
  onReload,
  onError,
}: {
  user: User
  domainName: string | undefined
  storageBytes: number | null | undefined
  onReload: () => void
  onError: (msg: string) => void
}) {
  const [changingPassword, setChangingPassword] = useState(false)
  const [newPassword, setNewPassword] = useState('')
  const [submitting, setSubmitting] = useState(false)
  const minLength = usePasswordMinLength()

  async function onSavePassword(e: FormEvent) {
    e.preventDefault()
    setSubmitting(true)
    try {
      await api.users.update(user.id, { password: newPassword })
      setNewPassword('')
      setChangingPassword(false)
    } catch (err) {
      onError(err instanceof ApiError ? err.message : 'Passwort ändern fehlgeschlagen')
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <>
      <tr>
        <td>
          {user.local_part}@{domainName}
        </td>
        <td>{user.role}</td>
        <td>
          <span className={`badge badge-${user.is_active ? 'ready' : 'not_ready'}`}>
            {user.is_active ? 'aktiv' : 'gesperrt'}
          </span>
        </td>
        <td className="muted">{formatBytes(storageBytes)}</td>
        <td style={{ display: 'flex', gap: '0.5rem' }}>
          <button onClick={() => setChangingPassword((v) => !v)}>
            {changingPassword ? 'Abbrechen' : 'Passwort ändern'}
          </button>
          <button
            className="btn-danger"
            onClick={() =>
              api.users
                .delete(user.id)
                .then(onReload)
                .catch((err) => onError(err instanceof ApiError ? err.message : 'Fehler'))
            }
          >
            Löschen
          </button>
        </td>
      </tr>
      {changingPassword && (
        <tr>
          <td colSpan={5}>
            <form className="inline-form" onSubmit={onSavePassword} style={{ margin: 0 }}>
              <input
                type="password"
                placeholder={`Neues Passwort (min. ${minLength} Zeichen)`}
                value={newPassword}
                onChange={(e) => setNewPassword(e.target.value)}
                required
                minLength={minLength}
                autoFocus
              />
              <button type="submit" disabled={submitting}>
                {submitting ? 'Speichere…' : 'Speichern'}
              </button>
            </form>
          </td>
        </tr>
      )}
    </>
  )
}

function UserForm({
  domainId,
  onCreated,
  onError,
}: {
  domainId: string
  onCreated: () => void
  onError: (msg: string) => void
}) {
  const [localPart, setLocalPart] = useState('')
  const [password, setPassword] = useState('')
  const minLength = usePasswordMinLength()

  async function onSubmit(e: FormEvent) {
    e.preventDefault()
    try {
      await api.users.create(domainId, localPart, password, 'user')
      setLocalPart('')
      setPassword('')
      onCreated()
    } catch (err) {
      onError(err instanceof ApiError ? err.message : 'Anlegen fehlgeschlagen')
    }
  }

  return (
    <form className="inline-form" onSubmit={onSubmit}>
      <input placeholder="postfach" value={localPart} onChange={(e) => setLocalPart(e.target.value)} required />
      <input
        type="password"
        placeholder={`Passwort (min. ${minLength} Zeichen)`}
        value={password}
        onChange={(e) => setPassword(e.target.value)}
        required
        minLength={minLength}
      />
      <button type="submit">Benutzer anlegen</button>
    </form>
  )
}

function CsvImportExport({
  domainId,
  domainName,
  onImported,
  onError,
}: {
  domainId: string
  domainName: string | undefined
  onImported: () => void
  onError: (msg: string) => void
}) {
  const [importing, setImporting] = useState(false)
  const [exporting, setExporting] = useState(false)
  const [result, setResult] = useState<ImportResponse | null>(null)

  async function onFileSelected(e: ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0]
    e.target.value = '' // erlaubt erneuten Import derselben Datei
    if (!file) return
    setImporting(true)
    setResult(null)
    try {
      const csv = await file.text()
      const response = await api.users.import(domainId, csv)
      setResult(response)
      if (response.created.length > 0) onImported()
    } catch (err) {
      onError(err instanceof ApiError ? err.message : 'Import fehlgeschlagen')
    } finally {
      setImporting(false)
    }
  }

  async function onExport() {
    setExporting(true)
    try {
      const csv = await api.users.exportCsv(domainId)
      const blob = new Blob([csv], { type: 'text/csv;charset=utf-8' })
      const url = URL.createObjectURL(blob)
      const a = document.createElement('a')
      a.href = url
      a.download = `postfaecher-${domainName ?? domainId}.csv`
      a.click()
      URL.revokeObjectURL(url)
    } catch (err) {
      onError(err instanceof ApiError ? err.message : 'Export fehlgeschlagen')
    } finally {
      setExporting(false)
    }
  }

  return (
    <div style={{ marginTop: '1rem', paddingTop: '1rem', borderTop: '1px solid var(--border)' }}>
      <h3 style={{ marginTop: 0 }}>CSV-Import/-Export</h3>
      <p className="muted">
        Import erwartet Kopfzeile <code>local_part,password,role,quota_bytes</code> (role/quota_bytes
        optional). Fehlerhafte Zeilen werden übersprungen, der Rest wird trotzdem angelegt.
      </p>
      <div style={{ display: 'flex', gap: '0.75rem', alignItems: 'center', flexWrap: 'wrap' }}>
        <label className="inline-form" style={{ display: 'inline-flex' }}>
          <input type="file" accept=".csv,text/csv" onChange={onFileSelected} disabled={importing} />
        </label>
        <button onClick={onExport} disabled={exporting}>
          {exporting ? 'Exportiere…' : 'Als CSV exportieren'}
        </button>
      </div>
      {result && (
        <div style={{ marginTop: '0.75rem' }}>
          <p>
            <span className="badge badge-ready">{result.created.length} angelegt</span>{' '}
            {result.errors.length > 0 && (
              <span className="badge badge-not_ready">{result.errors.length} übersprungen</span>
            )}
          </p>
          {result.errors.length > 0 && (
            <div className="table-wrap">
              <table className="data-table">
                <thead>
                  <tr>
                    <th>Zeile</th>
                    <th>local_part</th>
                    <th>Fehler</th>
                  </tr>
                </thead>
                <tbody>
                  {result.errors.map((e, i) => (
                    <tr key={i}>
                      <td>{e.row}</td>
                      <td>{e.local_part || '—'}</td>
                      <td>{e.message}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      )}
    </div>
  )
}

function AliasForm({
  domainId,
  onCreated,
  onError,
}: {
  domainId: string
  onCreated: () => void
  onError: (msg: string) => void
}) {
  const [source, setSource] = useState('')
  const [destination, setDestination] = useState('')

  async function onSubmit(e: FormEvent) {
    e.preventDefault()
    try {
      await api.aliases.create(domainId, source, [destination])
      setSource('')
      setDestination('')
      onCreated()
    } catch (err) {
      onError(err instanceof ApiError ? err.message : 'Anlegen fehlgeschlagen')
    }
  }

  return (
    <form className="inline-form" onSubmit={onSubmit}>
      <input placeholder="info" value={source} onChange={(e) => setSource(e.target.value)} required />
      <input
        placeholder="ziel@example.org"
        value={destination}
        onChange={(e) => setDestination(e.target.value)}
        required
      />
      <button type="submit">Alias anlegen</button>
    </form>
  )
}

function DnsSection({ domainId, onError }: { domainId: string; onError: (msg: string) => void }) {
  const [rec, setRec] = useState<DnsRecommendations | null>(null)
  const [checkResults, setCheckResults] = useState<DnsCheckResult[] | null>(null)
  const [checking, setChecking] = useState(false)

  function reload() {
    api.dns.recommendations(domainId).then(setRec).catch(() => {})
  }
  useEffect(reload, [domainId])

  async function onGenerateDkim() {
    try {
      await api.dns.generateDkim(domainId)
      reload()
    } catch (err) {
      onError(err instanceof ApiError ? err.message : 'DKIM-Erzeugung fehlgeschlagen')
    }
  }

  async function onCheck() {
    setChecking(true)
    try {
      const result = await api.dns.check(domainId)
      setCheckResults(result.results)
    } catch (err) {
      onError(err instanceof ApiError ? err.message : 'DNS-Prüfung fehlgeschlagen')
    } finally {
      setChecking(false)
    }
  }

  if (!rec) return null

  const entries = [rec.mx, rec.spf, rec.dkim, rec.dmarc].filter((e): e is NonNullable<typeof e> => e !== null)

  return (
    <section className="card">
      <h2>DNS-Einrichtung</h2>
      <p className="muted">
        Lege diese Einträge beim DNS-Provider der Domain an. Werte per Klick kopieren.
      </p>
      <div className="table-wrap">
        <table className="data-table">
          <thead>
            <tr>
              <th>Typ</th>
              <th>Name</th>
              <th>Wert</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {entries.map((e, i) => (
              <tr key={i}>
                <td>{e.record_type}</td>
                <td>
                  <code>{e.name}</code>
                </td>
                <td className="dns-value">
                  <code>{e.value}</code>
                </td>
                <td>
                  <button onClick={() => navigator.clipboard.writeText(e.value)}>Kopieren</button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {!rec.dkim && (
        <button onClick={onGenerateDkim} style={{ marginTop: '0.75rem' }}>
          DKIM-Schlüssel erzeugen
        </button>
      )}
      {rec.dkim && <DkimKeysSection domainId={domainId} onActivated={reload} onError={onError} />}
      <div style={{ marginTop: '1rem' }}>
        <button onClick={onCheck} disabled={checking}>
          {checking ? 'Prüfe…' : 'DNS jetzt prüfen'}
        </button>
        {checkResults && (
          <div className="table-wrap" style={{ marginTop: '0.75rem' }}>
            <table className="data-table">
              <thead>
                <tr>
                  <th>Typ</th>
                  <th>Erwartet</th>
                  <th>Gefunden</th>
                  <th>Status</th>
                </tr>
              </thead>
              <tbody>
                {checkResults.map((r, i) => (
                  <tr key={i}>
                    <td>{r.record_type}</td>
                    <td>
                      <code>{r.expected}</code>
                    </td>
                    <td>
                      <code>{r.actual ?? '—'}</code>
                    </td>
                    <td>
                      <span className={`badge badge-${r.status === 'ok' ? 'ready' : 'not_ready'}`}>
                        {r.status}
                      </span>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </section>
  )
}

function formatAgeDays(createdAt: string): number {
  return Math.floor((Date.now() - new Date(createdAt).getTime()) / (1000 * 60 * 60 * 24))
}

/**
 * DKIM-Schlüsselrotation (siehe routes/dns.rs): ein neu erzeugter Schlüssel
 * ersetzt den aktiven NICHT sofort, sondern startet "pending" — der Admin
 * veröffentlicht zuerst den neuen DNS-TXT-Eintrag und aktiviert den
 * Schlüssel erst danach (DNS-Propagation braucht Zeit; Empfänger mit noch
 * gecachtem altem öffentlichen Schlüssel sollen die bisherige Signatur
 * weiter validieren können).
 */
function DkimKeysSection({
  domainId,
  onActivated,
  onError,
}: {
  domainId: string
  onActivated: () => void
  onError: (msg: string) => void
}) {
  const [keys, setKeys] = useState<DkimKeyEntry[]>([])
  const [pendingDns, setPendingDns] = useState<{ name: string; value: string } | null>(null)
  const [generating, setGenerating] = useState(false)
  const [activating, setActivating] = useState<string | null>(null)

  function reload() {
    api.dns.listDkimKeys(domainId).then(setKeys).catch(() => {})
  }
  useEffect(reload, [domainId])

  async function onGenerateRotation() {
    setGenerating(true)
    try {
      const generated = await api.dns.generateDkim(domainId)
      if (!generated.active) {
        setPendingDns({ name: generated.dns_record_name, value: generated.dns_record_value })
      }
      reload()
    } catch (err) {
      onError(err instanceof ApiError ? err.message : 'DKIM-Erzeugung fehlgeschlagen')
    } finally {
      setGenerating(false)
    }
  }

  async function onActivate(selector: string) {
    setActivating(selector)
    try {
      await api.dns.activateDkimKey(domainId, selector)
      setPendingDns(null)
      reload()
      onActivated()
    } catch (err) {
      onError(err instanceof ApiError ? err.message : 'Aktivierung fehlgeschlagen')
    } finally {
      setActivating(null)
    }
  }

  const activeKey = keys.find((k) => k.active)

  return (
    <div style={{ marginTop: '1rem' }}>
      <h3 style={{ marginBottom: '0.25rem' }}>DKIM-Schlüssel</h3>
      {activeKey && (
        <p className="muted" style={{ marginTop: 0 }}>
          Aktiver Schlüssel <code>{activeKey.selector}</code> ist{' '}
          {formatAgeDays(activeKey.created_at)} Tage alt.
        </p>
      )}
      <div className="table-wrap">
        <table className="data-table">
          <thead>
            <tr>
              <th>Selektor</th>
              <th>Alter</th>
              <th>Status</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {keys.map((k) => (
              <tr key={k.selector}>
                <td>
                  <code>{k.selector}</code>
                </td>
                <td className="muted">{formatAgeDays(k.created_at)} Tage</td>
                <td>
                  <span className={`badge badge-${k.active ? 'ready' : 'not_ready'}`}>
                    {k.active ? 'aktiv' : 'ausstehend'}
                  </span>
                </td>
                <td>
                  {!k.active && (
                    <button
                      onClick={() => onActivate(k.selector)}
                      disabled={activating === k.selector}
                    >
                      {activating === k.selector ? 'Aktiviere…' : 'Aktivieren'}
                    </button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <button onClick={onGenerateRotation} disabled={generating} style={{ marginTop: '0.5rem' }}>
        {generating ? 'Erzeuge…' : 'Neuen Schlüssel erzeugen (Rotation)'}
      </button>
      {pendingDns && (
        <div className="card" style={{ marginTop: '0.75rem' }}>
          <p className="muted" style={{ marginTop: 0 }}>
            Neuer Schlüssel erzeugt, aber noch nicht aktiv. Zuerst diesen DNS-TXT-Eintrag anlegen
            und die Propagation abwarten, dann oben in der Tabelle aktivieren:
          </p>
          <p>
            <code>{pendingDns.name}</code>
          </p>
          <p className="dns-value">
            <code>{pendingDns.value}</code>
          </p>
          <button onClick={() => navigator.clipboard.writeText(pendingDns.value)}>Kopieren</button>
        </div>
      )}
    </div>
  )
}
