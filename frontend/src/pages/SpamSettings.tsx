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

  // Passwort-Richtlinie ist ein eigenes Feld auf derselben security_settings-
  // Zeile, aber ein eigener PATCH-Endpunkt (kein Rspamd-Reload nötig) — daher
  // eigener Form-State/Submit statt Mitschleppen im Score-Formular oben.
  const [minPasswordLength, setMinPasswordLength] = useState(12)
  const [pwError, setPwError] = useState<string | null>(null)
  const [pwSuccess, setPwSuccess] = useState(false)
  const [pwSubmitting, setPwSubmitting] = useState(false)

  // Webhook ist ebenfalls ein eigenes Feld derselben Zeile mit eigenem
  // PATCH-Endpunkt (kein Rspamd-Bezug) — dazu ein separater Testversand,
  // der die gerade eingegebene URL prüft, auch bevor sie gespeichert ist.
  const [webhookUrl, setWebhookUrl] = useState('')
  const [webhookEnabled, setWebhookEnabled] = useState(false)
  const [webhookError, setWebhookError] = useState<string | null>(null)
  const [webhookSuccess, setWebhookSuccess] = useState(false)
  const [webhookSubmitting, setWebhookSubmitting] = useState(false)
  const [testError, setTestError] = useState<string | null>(null)
  const [testSuccess, setTestSuccess] = useState(false)
  const [testing, setTesting] = useState(false)

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
        setMinPasswordLength(s.min_password_length)
        setWebhookUrl(s.webhook_url ?? '')
        setWebhookEnabled(s.webhook_enabled)
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

  async function onSubmitPasswordPolicy(e: FormEvent) {
    e.preventDefault()
    setPwError(null)
    setPwSuccess(false)
    setPwSubmitting(true)
    try {
      const updated = await api.securitySettings.updatePasswordPolicy(minPasswordLength)
      setSettings(updated)
      setPwSuccess(true)
    } catch (err) {
      setPwError(err instanceof ApiError ? err.message : 'Speichern fehlgeschlagen')
    } finally {
      setPwSubmitting(false)
    }
  }

  async function onSubmitWebhook(e: FormEvent) {
    e.preventDefault()
    setWebhookError(null)
    setWebhookSuccess(false)
    setWebhookSubmitting(true)
    try {
      const updated = await api.securitySettings.updateWebhook(
        webhookUrl.trim() === '' ? null : webhookUrl.trim(),
        webhookEnabled,
      )
      setSettings(updated)
      setWebhookSuccess(true)
    } catch (err) {
      setWebhookError(err instanceof ApiError ? err.message : 'Speichern fehlgeschlagen')
    } finally {
      setWebhookSubmitting(false)
    }
  }

  async function onTestWebhook() {
    setTestError(null)
    setTestSuccess(false)
    setTesting(true)
    try {
      await api.securitySettings.testWebhook(webhookUrl.trim())
      setTestSuccess(true)
    } catch (err) {
      setTestError(err instanceof ApiError ? err.message : 'Testversand fehlgeschlagen')
    } finally {
      setTesting(false)
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
      {!forbidden && settings && (
        <div className="card">
          <h2>Passwort-Richtlinie</h2>
          <p className="muted">
            Gilt für alle Postfächer domänenübergreifend — Selbstbedienung und Admin-Formulare
            lehnen kürzere Passwörter serverseitig ab, unabhängig vom jeweiligen Formular.
          </p>
          <form onSubmit={onSubmitPasswordPolicy}>
            <label>
              Mindestlänge
              <input
                type="number"
                min={8}
                value={minPasswordLength}
                onChange={(e) => setMinPasswordLength(Number(e.target.value))}
                required
              />
              <small>Mindestens 8 Zeichen — auch als hartes Limit in der Datenbank verankert.</small>
            </label>
            {pwError && (
              <p className="error" role="alert">
                {pwError}
              </p>
            )}
            {pwSuccess && <p className="badge badge-ready">Gespeichert</p>}
            <button type="submit" disabled={pwSubmitting}>
              {pwSubmitting ? 'Speichere…' : 'Speichern'}
            </button>
          </form>
        </div>
      )}
      {!forbidden && settings && (
        <div className="card">
          <h2>Webhook-Benachrichtigungen</h2>
          <p className="muted">
            Zweiter Kanal neben der Admin-E-Mail für Systemalarme (siehe{' '}
            <code>havenmail-cli notify-check</code>) — Slack-kompatibles JSON-Format, wird auch
            von Mattermost & vielen anderen Chat-Tools mit Incoming-Webhook akzeptiert.
          </p>
          <form onSubmit={onSubmitWebhook}>
            <label>
              Webhook-URL
              <input
                type="url"
                placeholder="https://hooks.example.org/services/…"
                value={webhookUrl}
                onChange={(e) => setWebhookUrl(e.target.value)}
              />
              <small>Muss mit https:// beginnen. Leer lassen, um den Webhook zu entfernen.</small>
            </label>
            <label className="checkbox-field">
              <input
                type="checkbox"
                checked={webhookEnabled}
                onChange={(e) => setWebhookEnabled(e.target.checked)}
              />
              Aktiv (erfordert eine gesetzte URL)
            </label>
            {webhookError && (
              <p className="error" role="alert">
                {webhookError}
              </p>
            )}
            {webhookSuccess && <p className="badge badge-ready">Gespeichert</p>}
            <div style={{ display: 'flex', gap: '0.75rem', alignItems: 'center', flexWrap: 'wrap' }}>
              <button type="submit" disabled={webhookSubmitting}>
                {webhookSubmitting ? 'Speichere…' : 'Speichern'}
              </button>
              <button
                type="button"
                disabled={testing || webhookUrl.trim() === ''}
                onClick={onTestWebhook}
              >
                {testing ? 'Sende…' : 'Test senden'}
              </button>
              {testSuccess && <span className="badge badge-ready">Testnachricht gesendet</span>}
            </div>
            {testError && (
              <p className="error" role="alert">
                {testError}
              </p>
            )}
          </form>
        </div>
      )}
    </div>
  )
}
