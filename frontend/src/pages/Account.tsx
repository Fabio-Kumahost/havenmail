import { useEffect, useState, type FormEvent } from 'react'
import { useNavigate } from 'react-router-dom'
import QRCode from 'qrcode'
import { api, ApiError, type SessionEntry, type ApiTokenEntry } from '../api'
import { useAuth } from '../AuthContext'

export default function Account() {
  return (
    <div>
      <h1>Mein Konto</h1>
      <PasswordSection />
      <TotpSection />
      <SessionsSection />
      <ApiTokensSection />
    </div>
  )
}

function PasswordSection() {
  const [currentPassword, setCurrentPassword] = useState('')
  const [newPassword, setNewPassword] = useState('')
  const [confirmPassword, setConfirmPassword] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [success, setSuccess] = useState(false)
  const [submitting, setSubmitting] = useState(false)
  // Server ist die eigentliche Durchsetzungsstelle (routes/users.rs liest
  // die Richtlinie live aus der DB) — hier nur für UX, damit das
  // `minLength`-Attribut nicht mehr "12" hart codiert, sondern die
  // tatsächlich vom super_admin konfigurierte Mindestlänge widerspiegelt.
  // 12 bleibt der Fallback, solange die Anfrage noch lädt.
  const [minLength, setMinLength] = useState(12)

  useEffect(() => {
    api.securitySettings
      .passwordPolicy()
      .then((p) => setMinLength(p.min_password_length))
      .catch(() => {})
  }, [])

  async function onSubmit(e: FormEvent) {
    e.preventDefault()
    setError(null)
    setSuccess(false)

    if (newPassword !== confirmPassword) {
      setError('Die neuen Passwörter stimmen nicht überein.')
      return
    }

    setSubmitting(true)
    try {
      await api.account.changePassword(currentPassword, newPassword)
      setSuccess(true)
      setCurrentPassword('')
      setNewPassword('')
      setConfirmPassword('')
    } catch (err) {
      setError(err instanceof ApiError ? err.message : 'Passwort konnte nicht geändert werden')
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <div className="card">
      <h2>Passwort ändern</h2>
      <form
        className="inline-form"
        onSubmit={onSubmit}
        style={{ flexDirection: 'column', alignItems: 'stretch', maxWidth: '24rem' }}
      >
        <label>
          Aktuelles Passwort
          <input
            type="password"
            value={currentPassword}
            onChange={(e) => setCurrentPassword(e.target.value)}
            required
            autoComplete="current-password"
          />
        </label>
        <label>
          Neues Passwort
          <input
            type="password"
            value={newPassword}
            onChange={(e) => setNewPassword(e.target.value)}
            required
            minLength={minLength}
            autoComplete="new-password"
          />
        </label>
        <p className="muted" style={{ margin: '-0.5rem 0 0' }}>
          Mindestens {minLength} Zeichen.
        </p>
        <label>
          Neues Passwort bestätigen
          <input
            type="password"
            value={confirmPassword}
            onChange={(e) => setConfirmPassword(e.target.value)}
            required
            minLength={minLength}
            autoComplete="new-password"
          />
        </label>
        {error && (
          <p className="error" role="alert">
            {error}
          </p>
        )}
        {success && <p className="badge badge-ready">Passwort geändert</p>}
        <button type="submit" disabled={submitting}>
          {submitting ? 'Ändere…' : 'Passwort ändern'}
        </button>
      </form>
    </div>
  )
}

function TotpSection() {
  const [enabled, setEnabled] = useState<boolean | null>(null)
  const [error, setError] = useState<string | null>(null)

  // Enrollment-Zwischenstand — lebt nur im Browser-Speicher dieser
  // Komponente, wird nie irgendwo abgelegt; erst ein erfolgreiches
  // confirm() macht daraus etwas Dauerhaftes (siehe routes/totp.rs).
  const [enrolling, setEnrolling] = useState(false)
  const [secret, setSecret] = useState('')
  const [qrDataUrl, setQrDataUrl] = useState<string | null>(null)
  const [code, setCode] = useState('')
  const [confirming, setConfirming] = useState(false)

  const [disabling, setDisabling] = useState(false)
  const [disablePassword, setDisablePassword] = useState('')
  const [submittingDisable, setSubmittingDisable] = useState(false)

  function reload() {
    api.totp
      .status()
      .then((s) => setEnabled(s.enabled))
      .catch(() => setError('Status konnte nicht geladen werden'))
  }
  useEffect(reload, [])

  async function onStartEnroll() {
    setError(null)
    try {
      const enrollment = await api.totp.enroll()
      setSecret(enrollment.secret)
      const dataUrl = await QRCode.toDataURL(enrollment.otpauth_uri)
      setQrDataUrl(dataUrl)
      setEnrolling(true)
    } catch (err) {
      setError(err instanceof ApiError ? err.message : '2FA-Einrichtung fehlgeschlagen')
    }
  }

  async function onConfirm(e: FormEvent) {
    e.preventDefault()
    setError(null)
    setConfirming(true)
    try {
      await api.totp.confirm(secret, code)
      setEnrolling(false)
      setSecret('')
      setQrDataUrl(null)
      setCode('')
      reload()
    } catch (err) {
      setError(err instanceof ApiError ? err.message : 'Code konnte nicht bestätigt werden')
    } finally {
      setConfirming(false)
    }
  }

  function onCancelEnroll() {
    setEnrolling(false)
    setSecret('')
    setQrDataUrl(null)
    setCode('')
    setError(null)
  }

  async function onDisable(e: FormEvent) {
    e.preventDefault()
    setError(null)
    setSubmittingDisable(true)
    try {
      await api.totp.disable(disablePassword)
      setDisabling(false)
      setDisablePassword('')
      reload()
    } catch (err) {
      setError(err instanceof ApiError ? err.message : 'Deaktivieren fehlgeschlagen')
    } finally {
      setSubmittingDisable(false)
    }
  }

  return (
    <div className="card">
      <h2>Zwei-Faktor-Authentifizierung (TOTP)</h2>
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      {enabled === null && <p className="muted">Lädt…</p>}

      {enabled === false && !enrolling && (
        <>
          <p className="muted">
            Beim Anmelden zusätzlich zum Passwort einen Code aus einer Authenticator-App verlangen
            (z. B. Google Authenticator, Authy, 1Password).
          </p>
          <button onClick={onStartEnroll}>Aktivieren</button>
        </>
      )}

      {enrolling && (
        <form onSubmit={onConfirm} style={{ maxWidth: '24rem' }}>
          <p>QR-Code in der Authenticator-App scannen:</p>
          {qrDataUrl && (
            <img
              src={qrDataUrl}
              alt="QR-Code für die TOTP-Einrichtung"
              width={200}
              height={200}
              style={{ background: '#fff', padding: '0.5rem', borderRadius: '0.5rem' }}
            />
          )}
          <p className="muted">
            Geht es nicht per Scan? Secret manuell eingeben: <code>{secret}</code>
          </p>
          <label>
            Bestätigungscode
            <input
              type="text"
              inputMode="numeric"
              pattern="[0-9]{6}"
              maxLength={6}
              value={code}
              onChange={(e) => setCode(e.target.value.replace(/\D/g, ''))}
              required
              autoFocus
            />
          </label>
          <div style={{ display: 'flex', gap: '0.5rem', marginTop: '0.75rem' }}>
            <button type="submit" disabled={confirming}>
              {confirming ? 'Bestätige…' : 'Bestätigen und aktivieren'}
            </button>
            <button type="button" onClick={onCancelEnroll}>
              Abbrechen
            </button>
          </div>
        </form>
      )}

      {enabled === true && !disabling && (
        <>
          <p>
            <span className="badge badge-ready">aktiv</span>
          </p>
          <button className="btn-danger" onClick={() => setDisabling(true)}>
            Deaktivieren
          </button>
        </>
      )}

      {disabling && (
        <form onSubmit={onDisable} style={{ maxWidth: '24rem' }}>
          <label>
            Passwort zur Bestätigung
            <input
              type="password"
              value={disablePassword}
              onChange={(e) => setDisablePassword(e.target.value)}
              required
              autoFocus
              autoComplete="current-password"
            />
          </label>
          <div style={{ display: 'flex', gap: '0.5rem', marginTop: '0.75rem' }}>
            <button type="submit" className="btn-danger" disabled={submittingDisable}>
              {submittingDisable ? 'Deaktiviere…' : 'Wirklich deaktivieren'}
            </button>
            <button
              type="button"
              onClick={() => {
                setDisabling(false)
                setDisablePassword('')
              }}
            >
              Abbrechen
            </button>
          </div>
        </form>
      )}
    </div>
  )
}

function SessionsSection() {
  const { logout } = useAuth()
  const navigate = useNavigate()
  const [sessions, setSessions] = useState<SessionEntry[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [revokingId, setRevokingId] = useState<string | null>(null)

  function reload() {
    api.sessions
      .list()
      .then(setSessions)
      .catch(() => setError('Sitzungen konnten nicht geladen werden'))
  }
  useEffect(reload, [])

  async function onRevoke(id: string) {
    setRevokingId(id)
    setError(null)
    try {
      const result = await api.sessions.revoke(id)
      if (result.was_current) {
        // Der eigene Zugriff wurde gerade widerrufen — sessionStorage
        // lokal aufräumen und zurück zum Login, statt mit einem toten
        // Refresh-Token weiterzumachen.
        logout()
        navigate('/login')
        return
      }
      reload()
    } catch (err) {
      setError(err instanceof ApiError ? err.message : 'Abmelden fehlgeschlagen')
    } finally {
      setRevokingId(null)
    }
  }

  return (
    <div className="card">
      <h2>Aktive Sitzungen</h2>
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      {!sessions && !error && <p className="muted">Lädt…</p>}
      {sessions && sessions.length === 0 && <p className="muted">Keine aktiven Sitzungen.</p>}
      {sessions && sessions.length > 0 && (
        <table className="data-table">
          <thead>
            <tr>
              <th>IP-Adresse</th>
              <th>Gerät/Browser</th>
              <th>Angemeldet seit</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {sessions.map((s) => (
              <tr key={s.id}>
                <td>
                  <code>{s.ip ?? '—'}</code>
                </td>
                <td className="muted" style={{ maxWidth: '20rem', overflow: 'hidden', textOverflow: 'ellipsis' }}>
                  {s.user_agent ?? '—'}
                </td>
                <td>
                  {new Date(s.created_at).toLocaleString('de-DE')}
                  {s.is_current && (
                    <>
                      {' '}
                      <span className="badge badge-ready">diese Sitzung</span>
                    </>
                  )}
                </td>
                <td>
                  <button
                    className="btn-danger"
                    onClick={() => onRevoke(s.id)}
                    disabled={revokingId === s.id}
                  >
                    {revokingId === s.id ? 'Melde ab…' : 'Abmelden'}
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  )
}

function ApiTokensSection() {
  const [tokens, setTokens] = useState<ApiTokenEntry[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [revokingId, setRevokingId] = useState<string | null>(null)

  const [scopesInput, setScopesInput] = useState('')
  const [expiresInDays, setExpiresInDays] = useState('')
  const [creating, setCreating] = useState(false)
  const [justCreated, setJustCreated] = useState<string | null>(null)

  function reload() {
    api.apiTokens
      .list()
      .then(setTokens)
      .catch(() => setError('API-Keys konnten nicht geladen werden'))
  }
  useEffect(reload, [])

  async function onCreate(e: FormEvent) {
    e.preventDefault()
    setError(null)
    setCreating(true)
    try {
      const scopes = scopesInput
        .split(',')
        .map((s) => s.trim())
        .filter(Boolean)
      const days = expiresInDays ? Number(expiresInDays) : undefined
      const created = await api.apiTokens.create(scopes, days)
      setJustCreated(created.token)
      setScopesInput('')
      setExpiresInDays('')
      reload()
    } catch (err) {
      setError(err instanceof ApiError ? err.message : 'API-Key konnte nicht erzeugt werden')
    } finally {
      setCreating(false)
    }
  }

  async function onRevoke(id: string) {
    setRevokingId(id)
    setError(null)
    try {
      await api.apiTokens.revoke(id)
      reload()
    } catch (err) {
      setError(err instanceof ApiError ? err.message : 'Widerrufen fehlgeschlagen')
    } finally {
      setRevokingId(null)
    }
  }

  return (
    <div className="card">
      <h2>API-Keys für Automatisierung</h2>
      <p className="muted">
        Für Skripte/CI, die die Havenmail-API ohne Passwort ansprechen sollen — hat dieselben
        Rechte wie dieses Konto und lässt sich einzeln widerrufen.
      </p>
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}

      {justCreated && (
        <div className="badge badge-ready" style={{ display: 'block', padding: '0.75rem', marginBottom: '1rem' }}>
          <p style={{ margin: '0 0 0.5rem 0' }}>
            Key erzeugt — wird nur jetzt einmalig angezeigt, danach nicht mehr abrufbar:
          </p>
          <code style={{ userSelect: 'all', wordBreak: 'break-all' }}>{justCreated}</code>
          <div style={{ marginTop: '0.5rem' }}>
            <button type="button" onClick={() => setJustCreated(null)}>
              Verstanden, ausblenden
            </button>
          </div>
        </div>
      )}

      <form
        className="inline-form"
        onSubmit={onCreate}
        style={{ flexDirection: 'column', alignItems: 'stretch', maxWidth: '24rem' }}
      >
        <label>
          Label(s), kommagetrennt
          <input
            type="text"
            placeholder="ci-deploy, monitoring"
            value={scopesInput}
            onChange={(e) => setScopesInput(e.target.value)}
          />
        </label>
        <label>
          Läuft ab nach (Tage, leer = nie)
          <input
            type="number"
            min={1}
            value={expiresInDays}
            onChange={(e) => setExpiresInDays(e.target.value)}
          />
        </label>
        <button type="submit" disabled={creating}>
          {creating ? 'Erzeuge…' : 'Neuen API-Key erzeugen'}
        </button>
      </form>

      {tokens && tokens.length > 0 && (
        <table className="data-table" style={{ marginTop: '1rem' }}>
          <thead>
            <tr>
              <th>Label(s)</th>
              <th>Erzeugt</th>
              <th>Läuft ab</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {tokens.map((t) => (
              <tr key={t.id}>
                <td>{t.scopes.length > 0 ? t.scopes.join(', ') : '—'}</td>
                <td>{new Date(t.created_at).toLocaleString('de-DE')}</td>
                <td>{t.expires_at ? new Date(t.expires_at).toLocaleString('de-DE') : 'nie'}</td>
                <td>
                  <button
                    className="btn-danger"
                    onClick={() => onRevoke(t.id)}
                    disabled={revokingId === t.id}
                  >
                    {revokingId === t.id ? 'Widerrufe…' : 'Widerrufen'}
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {tokens && tokens.length === 0 && <p className="muted">Noch keine API-Keys.</p>}
    </div>
  )
}
