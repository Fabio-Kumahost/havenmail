import { useEffect, useState, type FormEvent } from 'react'
import { useParams } from 'react-router-dom'
import {
  api,
  type Domain,
  type User,
  type Alias,
  type DnsRecommendations,
  type DnsCheckResult,
  ApiError,
} from '../api'

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
      </section>

      <section className="card">
        <h2>Aliase</h2>
        <AliasForm domainId={domainId} onCreated={reload} onError={setError} />
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
      </section>

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
                placeholder="Neues Passwort (min. 12 Zeichen)"
                value={newPassword}
                onChange={(e) => setNewPassword(e.target.value)}
                required
                minLength={12}
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
        placeholder="Passwort (min. 12 Zeichen)"
        value={password}
        onChange={(e) => setPassword(e.target.value)}
        required
        minLength={12}
      />
      <button type="submit">Benutzer anlegen</button>
    </form>
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
      {!rec.dkim && (
        <button onClick={onGenerateDkim} style={{ marginTop: '0.75rem' }}>
          DKIM-Schlüssel erzeugen
        </button>
      )}
      <div style={{ marginTop: '1rem' }}>
        <button onClick={onCheck} disabled={checking}>
          {checking ? 'Prüfe…' : 'DNS jetzt prüfen'}
        </button>
        {checkResults && (
          <table className="data-table" style={{ marginTop: '0.75rem' }}>
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
        )}
      </div>
    </section>
  )
}
