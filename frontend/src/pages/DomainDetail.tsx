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

export default function DomainDetail() {
  const { domainId } = useParams<{ domainId: string }>()
  const [domain, setDomain] = useState<Domain | null>(null)
  const [users, setUsers] = useState<User[]>([])
  const [aliases, setAliases] = useState<Alias[]>([])
  const [error, setError] = useState<string | null>(null)

  function reload() {
    if (!domainId) return
    api.domains.get(domainId).then(setDomain).catch(() => {})
    api.users.list(domainId).then(setUsers).catch(() => {})
    api.aliases.list(domainId).then(setAliases).catch(() => {})
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
              <th></th>
            </tr>
          </thead>
          <tbody>
            {users.map((u) => (
              <tr key={u.id}>
                <td>
                  {u.local_part}@{domain?.name}
                </td>
                <td>{u.role}</td>
                <td>
                  <span className={`badge badge-${u.is_active ? 'ready' : 'not_ready'}`}>
                    {u.is_active ? 'aktiv' : 'gesperrt'}
                  </span>
                </td>
                <td>
                  <button
                    onClick={() =>
                      api.users
                        .delete(u.id)
                        .then(reload)
                        .catch((err) => setError(err instanceof ApiError ? err.message : 'Fehler'))
                    }
                  >
                    Löschen
                  </button>
                </td>
              </tr>
            ))}
            {users.length === 0 && (
              <tr>
                <td colSpan={4} className="muted">
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
