import { useState, type FormEvent } from 'react'
import { useNavigate } from 'react-router-dom'
import { useAuth, ApiError } from '../AuthContext'
import { useBranding } from '../BrandingContext'

export default function Login() {
  const { login } = useAuth()
  const { branding } = useBranding()
  const navigate = useNavigate()
  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')
  const [totpCode, setTotpCode] = useState('')
  const [totpRequired, setTotpRequired] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [submitting, setSubmitting] = useState(false)

  async function onSubmit(e: FormEvent) {
    e.preventDefault()
    setError(null)
    setSubmitting(true)
    try {
      const result = await login(email, password, totpRequired ? totpCode : undefined)
      if (result === 'totp_required') {
        setTotpRequired(true)
        if (totpCode) {
          setError('Code ist ungültig oder abgelaufen.')
          setTotpCode('')
        }
      } else {
        navigate('/')
      }
    } catch (err) {
      setError(err instanceof ApiError ? err.message : 'Anmeldung fehlgeschlagen')
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <div className="auth-screen">
      <form className="auth-card" onSubmit={onSubmit}>
        {branding.logo_url && (
          <img
            src={branding.logo_url}
            alt={branding.product_name}
            style={{ maxHeight: '3rem', maxWidth: '100%', marginBottom: '0.5rem' }}
          />
        )}
        <h1>{branding.product_name} Admin</h1>
        <label>
          E-Mail
          <input
            type="email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            required
            autoFocus={!totpRequired}
            disabled={totpRequired}
          />
        </label>
        <label>
          Passwort
          <input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            required
            disabled={totpRequired}
          />
        </label>
        {totpRequired && (
          <label>
            Bestätigungscode aus der Authenticator-App
            <input
              type="text"
              inputMode="numeric"
              pattern="[0-9]{6}"
              maxLength={6}
              value={totpCode}
              onChange={(e) => setTotpCode(e.target.value.replace(/\D/g, ''))}
              required
              autoFocus
            />
          </label>
        )}
        {error && (
          <p className="error" role="alert">
            {error}
          </p>
        )}
        <button type="submit" disabled={submitting}>
          {submitting ? 'Anmelden…' : totpRequired ? 'Bestätigen' : 'Anmelden'}
        </button>
      </form>
    </div>
  )
}
