import { useState, type FormEvent } from 'react'
import { api, ApiError } from '../api'

export default function Account() {
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
    <div>
      <h1>Mein Konto</h1>
      <div className="card">
        <h2>Passwort ändern</h2>
        <form className="inline-form" onSubmit={onSubmit} style={{ flexDirection: 'column', alignItems: 'stretch', maxWidth: '24rem' }}>
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
    </div>
  )
}
