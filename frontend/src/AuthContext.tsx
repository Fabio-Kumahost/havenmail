import { createContext, useContext, useState, type ReactNode } from 'react'
import { api, setTokens, clearTokens, isAuthenticated, isTotpRequired, ApiError } from './api'

interface AuthContextValue {
  loggedIn: boolean
  /** Gibt 'totp_required' zurück statt einzuloggen, wenn das Konto 2FA
   *  aktiviert hat und kein (oder ein falscher) totpCode mitgegeben wurde —
   *  der Aufrufer (Login.tsx) zeigt dann ein Code-Eingabefeld und ruft
   *  login() erneut mit totpCode auf. */
  login: (email: string, password: string, totpCode?: string) => Promise<'ok' | 'totp_required'>
  logout: () => void
}

const AuthContext = createContext<AuthContextValue | null>(null)

export function AuthProvider({ children }: { children: ReactNode }) {
  // isAuthenticated() liest Tokens, die api.ts beim Modul-Start aus
  // sessionStorage wiederhergestellt hat — sonst würde jeder Reload
  // fälschlich als "ausgeloggt" starten, bevor RequireAuth überhaupt
  // einen API-Call machen konnte.
  const [loggedIn, setLoggedIn] = useState(isAuthenticated())

  async function login(email: string, password: string, totpCode?: string) {
    const result = await api.login(email, password, totpCode)
    if (isTotpRequired(result)) {
      return 'totp_required' as const
    }
    setTokens(result.access_token, result.refresh_token)
    setLoggedIn(true)
    return 'ok' as const
  }

  function logout() {
    clearTokens()
    setLoggedIn(false)
  }

  return <AuthContext.Provider value={{ loggedIn, login, logout }}>{children}</AuthContext.Provider>
}

export function useAuth() {
  const ctx = useContext(AuthContext)
  if (!ctx) throw new Error('useAuth muss innerhalb von AuthProvider verwendet werden')
  return ctx
}

export { ApiError }
