import { useState, type FormEvent } from 'react'
import { api, ApiError } from '../api'
import { useBranding } from '../BrandingContext'

export default function Branding() {
  const { branding, refresh } = useBranding()
  const [form, setForm] = useState({
    product_name: branding.product_name,
    logo_url: branding.logo_url ?? '',
    accent_color: branding.accent_color ?? '',
  })
  const [error, setError] = useState<string | null>(null)
  const [forbidden, setForbidden] = useState(false)
  const [success, setSuccess] = useState(false)
  const [submitting, setSubmitting] = useState(false)

  async function onSubmit(e: FormEvent) {
    e.preventDefault()
    setError(null)
    setSuccess(false)
    setSubmitting(true)
    try {
      await api.branding.update({
        product_name: form.product_name,
        logo_url: form.logo_url || null,
        accent_color: form.accent_color || null,
      })
      refresh()
      setSuccess(true)
    } catch (err) {
      if (err instanceof ApiError && err.status === 403) {
        setForbidden(true)
      } else {
        setError(err instanceof ApiError ? err.message : 'Speichern fehlgeschlagen')
      }
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <div>
      <h1>Branding</h1>
      {forbidden && (
        <div className="card">
          <p className="badge badge-not_ready">Nur für Administratoren mit vollem Systemzugriff sichtbar.</p>
        </div>
      )}
      {!forbidden && (
        <div className="card">
          <p className="muted">
            Produktname, Logo und Akzentfarbe des Panels — sichtbar für alle Nutzer, auch auf der
            Login-Seite vor der Anmeldung. Die Akzentfarbe gilt für Light- und Dark-Mode
            gleichermaßen (kein separater Wert je Modus).
          </p>
          <form onSubmit={onSubmit} style={{ maxWidth: '28rem' }}>
            <label>
              Produktname
              <input
                type="text"
                value={form.product_name}
                onChange={(e) => setForm({ ...form, product_name: e.target.value })}
                required
                maxLength={60}
              />
            </label>
            <label>
              Logo-URL (optional)
              <input
                type="url"
                placeholder="https://beispiel.org/logo.png"
                value={form.logo_url}
                onChange={(e) => setForm({ ...form, logo_url: e.target.value })}
              />
              <small>Muss bereits gehostet sein — es gibt keinen Datei-Upload, nur eine URL.</small>
            </label>
            <label>
              Akzentfarbe (optional)
              <input
                type="text"
                placeholder="#4f46e5"
                value={form.accent_color}
                onChange={(e) => setForm({ ...form, accent_color: e.target.value })}
              />
              <small>Jeder gültige CSS-Farbwert (Hex, rgb(), hsl(), …). Leer = Standardfarbe.</small>
            </label>

            {form.logo_url && (
              <div style={{ margin: '1rem 0' }}>
                <p className="muted" style={{ marginBottom: '0.25rem' }}>
                  Vorschau:
                </p>
                <img
                  src={form.logo_url}
                  alt="Logo-Vorschau"
                  style={{ maxHeight: '3rem', maxWidth: '100%' }}
                  onError={(e) => {
                    ;(e.target as HTMLImageElement).style.display = 'none'
                  }}
                />
              </div>
            )}

            {error && (
              <p className="error" role="alert">
                {error}
              </p>
            )}
            {success && <p className="badge badge-ready">Gespeichert</p>}
            <button type="submit" disabled={submitting}>
              {submitting ? 'Speichere…' : 'Speichern'}
            </button>
          </form>
        </div>
      )}
    </div>
  )
}
