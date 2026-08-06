import { useEffect, useState, type FormEvent } from 'react'
import QRCode from 'qrcode'
import { api, ApiError } from '../api'

export default function Account() {
  return (
    <div>
      <h1>Mein Konto</h1>
      <PasswordSection />
      <TotpSection />
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
            minLength={12}
            autoComplete="new-password"
          />
        </label>
        <label>
          Neues Passwort bestätigen
          <input
            type="password"
            value={confirmPassword}
            onChange={(e) => setConfirmPassword(e.target.value)}
            required
            minLength={12}
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
