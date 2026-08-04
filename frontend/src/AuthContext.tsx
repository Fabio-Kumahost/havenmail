import { createContext, useContext, useState, type ReactNode } from 'react'
import { api, setTokens, clearTokens, ApiError } from './api'

interface AuthContextValue {
  loggedIn: boolean
  login: (email: string, password: string) => Promise<void>
  logout: () => void
}

const AuthContext = createContext<AuthContextValue | null>(null)

export function AuthProvider({ children }: { children: ReactNode }) {
  const [loggedIn, setLoggedIn] = useState(false)

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
