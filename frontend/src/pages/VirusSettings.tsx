import { useEffect, useState, type FormEvent } from 'react'
import { api, ApiError, type SecuritySettings } from '../api'

export default function VirusSettings() {
  const [settings, setSettings] = useState<SecuritySettings | null>(null)
  const [form, setForm] = useState({
    antivirus_enabled: true,
    antivirus_action: 'reject' as SecuritySettings['antivirus_action'],
    antivirus_max_size_mb: 25,
  })
  const [error, setError] = useState<string | null>(null)
  const [forbidden, setForbidden] = useState(false)
  const [success, setSuccess] = useState(false)
  const [submitting, setSubmitting] = useState(false)

  useEffect(() => {
    api.securitySettings
      .get()
      .then((s) => {
        setSettings(s)
        setForm({
          antivirus_enabled: s.antivirus_enabled,
          antivirus_action: s.antivirus_action,
          antivirus_max_size_mb: s.antivirus_max_size_mb,
        })
      })
      .catch((err: unknown) => {
        if (err instanceof ApiError && err.status === 403) {
          setForbidden(true)
        } else {
          setError('Einstellungen konnten nicht geladen werden.')
        }
      })
  }, [])

  async function onSubmit(e: FormEvent) {
    e.preventDefault()
    setError(null)
    setSuccess(false)
    setSubmitting(true)
    try {
      const updated = await api.securitySettings.updateVirus(form)
      setSettings(updated)
      setSuccess(true)
    } catch (err) {
      setError(err instanceof ApiError ? err.message : 'Speichern fehlgeschlagen')
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <div>
      <h1>Virenschutz</h1>
      {forbidden && (
        <div className="card">
          <p className="badge badge-not_ready">Nur für Administratoren mit vollem Systemzugriff sichtbar.</p>
        </div>
      )}
      {!forbidden && !settings && !error && (
        <div className="card">
          <p className="muted">Lädt…</p>
        </div>
      )}
      {!forbidden && settings && (
        <div className="card">
          <h2>ClamAV-Anbindung</h2>
          <p className="muted">
            Jede Mail wird über Rspamd an ClamAV zur Virenprüfung weitergereicht.
          </p>
          <form onSubmit={onSubmit}>
            <label className="checkbox-field">
              <input
                type="checkbox"
                checked={form.antivirus_enabled}
                onChange={(e) => setForm({ ...form, antivirus_enabled: e.target.checked })}
              />
              Virenprüfung aktiv
            </label>
            <label>
              Bei Fund
              <select
                value={form.antivirus_action}
                onChange={(e) =>
                  setForm({
                    ...form,
                    antivirus_action: e.target.value as SecuritySettings['antivirus_action'],
                  })
                }
              >
                <option value="reject">Mail hart ablehnen</option>
                <option value="add_header">Nur markieren (Header hinzufügen)</option>
                <option value="no_action">Nur protokollieren, nicht eingreifen</option>
              </select>
            </label>
            <label>
              Maximale Prüfgröße (MB)
              <input
                type="number"
                min={1}
                value={form.antivirus_max_size_mb}
                onChange={(e) =>
                  setForm({ ...form, antivirus_max_size_mb: Number(e.target.value) })
                }
                required
              />
              <small>Größere Anhänge werden ungeprüft zugestellt.</small>
            </label>

            {error && (
              <p className="error" role="alert">
                {error}
              </p>
            )}
            {success && <p className="badge badge-ready">Gespeichert und angewendet</p>}
            <button type="submit" disabled={submitting}>
              {submitting ? 'Speichere…' : 'Speichern'}
            </button>
          </form>
        </div>
      )}
    </div>
  )
}
