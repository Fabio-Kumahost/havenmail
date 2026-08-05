import { useEffect, useState, type FormEvent } from 'react'
import { api, ApiError, type SecuritySettings } from '../api'

export default function SpamSettings() {
  const [settings, setSettings] = useState<SecuritySettings | null>(null)
  const [form, setForm] = useState({
    spam_greylist_score: 4,
    spam_add_header_score: 6,
    spam_reject_score: 15,
    dmarc_enabled: true,
    ratelimit_enabled: true,
    ratelimit_per_hour: 100,
    ratelimit_burst: 100,
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
          spam_greylist_score: s.spam_greylist_score,
          spam_add_header_score: s.spam_add_header_score,
          spam_reject_score: s.spam_reject_score,
          dmarc_enabled: s.dmarc_enabled,
          ratelimit_enabled: s.ratelimit_enabled,
          ratelimit_per_hour: s.ratelimit_per_hour,
          ratelimit_burst: s.ratelimit_burst,
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
      const updated = await api.securitySettings.updateSpam(form)
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
      <h1>Spam-Schutz</h1>
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
          <h2>Score-Schwellen</h2>
          <p className="muted">
            Rspamd bewertet jede Mail mit einer Punktzahl. Ab welcher Punktzahl was passiert,
            legst du hier fest — die Werte müssen aufsteigend sein.
          </p>
          <form onSubmit={onSubmit}>
            <label>
              Graylisting ab Score
              <input
                type="number"
                step="0.5"
                value={form.spam_greylist_score}
                onChange={(e) => setForm({ ...form, spam_greylist_score: Number(e.target.value) })}
                required
              />
              <small>Verdächtige Mail wird kurz zurückgestellt und beim erneuten Versuch akzeptiert.</small>
            </label>
            <label>
              Spam-Header hinzufügen ab Score
              <input
                type="number"
                step="0.5"
                value={form.spam_add_header_score}
                onChange={(e) => setForm({ ...form, spam_add_header_score: Number(e.target.value) })}
                required
              />
              <small>Mail wird zugestellt, aber als Spam markiert (X-Spam-Header).</small>
            </label>
            <label>
              Ablehnen ab Score
              <input
                type="number"
                step="0.5"
                value={form.spam_reject_score}
                onChange={(e) => setForm({ ...form, spam_reject_score: Number(e.target.value) })}
                required
              />
              <small>Mail wird beim Empfang hart abgelehnt.</small>
            </label>

            <h2>DMARC-Auswertung</h2>
            <label className="checkbox-field">
              <input
                type="checkbox"
                checked={form.dmarc_enabled}
                onChange={(e) => setForm({ ...form, dmarc_enabled: e.target.checked })}
              />
              DMARC-Prüfung eingehender Mail aktiv
            </label>

            <h2>Rate-Limiting</h2>
            <label className="checkbox-field">
              <input
                type="checkbox"
                checked={form.ratelimit_enabled}
                onChange={(e) => setForm({ ...form, ratelimit_enabled: e.target.checked })}
              />
              Rate-Limiting für authentifizierte Postfächer aktiv
            </label>
            <label>
              Mails pro Stunde
              <input
                type="number"
                min={1}
                value={form.ratelimit_per_hour}
                onChange={(e) => setForm({ ...form, ratelimit_per_hour: Number(e.target.value) })}
                required
              />
            </label>
            <label>
              Burst (kurzfristige Spitze)
              <input
                type="number"
                min={1}
                value={form.ratelimit_burst}
                onChange={(e) => setForm({ ...form, ratelimit_burst: Number(e.target.value) })}
                required
              />
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
