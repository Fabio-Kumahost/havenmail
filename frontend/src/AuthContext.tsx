import { createContext, useContext, useState, type ReactNode } from 'react'
import { api, setTokens, clearTokens, isAuthenticated, ApiError } from './api'

interface AuthContextValue {
  loggedIn: boolean
  login: (email: string, password: string) => Promise<void>
  logout: () => void
}

const AuthContext = createContext<AuthContextValue | null>(null)

export function AuthProvider({ children }: { children: ReactNode }) {
  // isAuthenticated() liest Tokens, die api.ts beim Modul-Start aus
  // sessionStorage wiederhergestellt hat — sonst würde jeder Reload
  // fälschlich als "ausgeloggt" starten, bevor RequireAuth überhaupt
  // einen API-Call machen konnte.
  const [loggedIn, setLoggedIn] = useState(isAuthenticated())

  async function login(email: string, password: string) {
    const tokens = await api.login(email, password)
    setTokens(tokens.access_token, tokens.refresh_token)
    setLoggedIn(true)
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
