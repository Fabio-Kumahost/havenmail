import { useEffect, useState, type FormEvent } from 'react'
import { Link } from 'react-router-dom'
import { api, type Domain, ApiError } from '../api'

export default function Domains() {
  const [domains, setDomains] = useState<Domain[]>([])
  const [newName, setNewName] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)

  function reload() {
    api.domains
      .list()
      .then(setDomains)
      .catch((err) => setError(err instanceof ApiError ? err.message : 'Laden fehlgeschlagen'))
      .finally(() => setLoading(false))
  }

  useEffect(reload, [])

  async function onCreate(e: FormEvent) {
    e.preventDefault()
    setError(null)
    try {
      await api.domains.create(newName.trim())
      setNewName('')
      reload()
    } catch (err) {
      setError(err instanceof ApiError ? err.message : 'Anlegen fehlgeschlagen')
    }
  }

  return (
    <div>
      <h1>Domains</h1>
      <form className="inline-form" onSubmit={onCreate}>
        <input
          placeholder="beispiel.org"
          value={newName}
          onChange={(e) => setNewName(e.target.value)}
          required
        />
        <button type="submit">Domain anlegen</button>
      </form>
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      {loading ? (
        <p>Lädt…</p>
      ) : (
        <div className="table-wrap">
          <table className="data-table">
            <thead>
              <tr>
                <th>Name</th>
                <th>Status</th>
                <th>Catch-all</th>
              </tr>
            </thead>
            <tbody>
              {domains.map((d) => (
                <tr key={d.id}>
                  <td>
                    <Link to={`/domains/${d.id}`}>{d.name}</Link>
                  </td>
                  <td>
                    <span className={`badge badge-${d.is_active ? 'ready' : 'not_ready'}`}>
                      {d.is_active ? 'aktiv' : 'inaktiv'}
                    </span>
                  </td>
                  <td>{d.catch_all_enabled ? d.catch_all_target : '—'}</td>
                </tr>
              ))}
              {domains.length === 0 && (
                <tr>
                  <td colSpan={3} className="muted">
                    Keine Domains vorhanden.
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      )}
    </div>
  )
}
