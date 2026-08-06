import { createContext, useContext, useEffect, useState, type ReactNode } from 'react'
import { api, type BrandingSettings } from './api'

const DEFAULT_BRANDING: BrandingSettings = {
  product_name: 'Havenmail',
  logo_url: null,
  accent_color: null,
}

interface BrandingContextValue {
  branding: BrandingSettings
  /** Nach einer Änderung auf der Branding-Einstellungsseite aufrufen,
   *  damit Sidebar/Login-Vorschau sofort den neuen Stand zeigen, ohne
   *  Reload. */
  refresh: () => void
}

const BrandingContext = createContext<BrandingContextValue | null>(null)

/**
 * Lädt Produktname/Logo/Akzentfarbe einmal beim App-Start — auch auf der
 * Login-Seite (der Endpunkt ist bewusst öffentlich, siehe
 * routes/branding.rs), damit das Panel schon vor der Anmeldung
 * gebrandet aussieht, nicht erst danach. `accent_color` wird als
 * CSS-Custom-Property auf <html> gesetzt (überschreibt den Default aus
 * App.css für Light+Dark gleichermaßen) statt eine zweite Style-Quelle
 * einzuführen.
 */
export function BrandingProvider({ children }: { children: ReactNode }) {
  const [branding, setBranding] = useState<BrandingSettings>(DEFAULT_BRANDING)

  function reload() {
    api.branding
      .get()
      .then(setBranding)
      .catch(() => {
        // Branding ist rein kosmetisch — ein Ladefehler (z. B. API noch
        // nicht erreichbar) soll die App nicht blockieren, nur beim
        // Default bleiben.
      })
  }

  useEffect(reload, [])

  useEffect(() => {
    document.title = branding.product_name
    if (branding.accent_color) {
      document.documentElement.style.setProperty('--accent', branding.accent_color)
    } else {
      document.documentElement.style.removeProperty('--accent')
    }
  }, [branding])

  return (
    <BrandingContext.Provider value={{ branding, refresh: reload }}>
      {children}
    </BrandingContext.Provider>
  )
}

export function useBranding() {
  const ctx = useContext(BrandingContext)
  if (!ctx) throw new Error('useBranding muss innerhalb von BrandingProvider verwendet werden')
  return ctx
}
