/**
 * Havenmail Admin-Oberfläche.
 *
 * STATUS (M5): Login, Dashboard (Health-Status), Domain-Verwaltung,
 * Benutzer-/Alias-CRUD, DNS-Einrichtungsassistent (kopierbare Einträge +
 * Live-Prüfung), DKIM-Schlüsselerzeugung, System-Seite (Dienststatus der
 * Mail-Engines). Weiterhin offen: Quotas-Übersicht, Warteschlangen,
 * Zustellfehler, Spam-/Virenereignisse, TLS-Zertifikatslaufzeit,
 * Audit-Protokoll-Ansicht, Backup-/Update-Status — benötigen weitere
 * Backend-Endpunkte. Siehe CHANGELOG.md für den genauen Stand.
 */
import { BrowserRouter, Routes, Route } from 'react-router-dom'
import { AuthProvider } from './AuthContext'
import RequireAuth from './pages/RequireAuth'
import Layout from './pages/Layout'
import Login from './pages/Login'
import Dashboard from './pages/Dashboard'
import Domains from './pages/Domains'
import DomainDetail from './pages/DomainDetail'
import System from './pages/System'
import './App.css'

function App() {
  return (
    <AuthProvider>
      <BrowserRouter>
        <Routes>
          <Route path="/login" element={<Login />} />
          <Route element={<RequireAuth />}>
            <Route element={<Layout />}>
              <Route path="/" element={<Dashboard />} />
              <Route path="/domains" element={<Domains />} />
              <Route path="/domains/:domainId" element={<DomainDetail />} />
              <Route path="/system" element={<System />} />
            </Route>
          </Route>
        </Routes>
      </BrowserRouter>
    </AuthProvider>
  )
}

export default App
