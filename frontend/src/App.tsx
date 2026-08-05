/**
 * Havenmail Admin-Oberfläche.
 *
 * STATUS: Login, Dashboard (Health-Status + Verlaufscharts), Domain-
 * Verwaltung, Benutzer-/Alias-CRUD, DNS-Einrichtungsassistent (kopierbare
 * Einträge + Live-Prüfung), DKIM-Schlüsselerzeugung, System-Seite
 * (Dienststatus der Mail-Engines), Audit-Log-Ansicht (domänen-gescoped für
 * domain_admin), Spam-/Virenschutz-Einstellungen, Selbstbedienungs-
 * Passwortänderung. Weiterhin offen: Quotas-Übersicht, Zustellfehler-
 * Auswertung, Backup-/Update-Status — benötigen weitere Backend-Endpunkte.
 * Siehe CHANGELOG.md für den genauen Stand.
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
import AuditLog from './pages/AuditLog'
import SpamSettings from './pages/SpamSettings'
import VirusSettings from './pages/VirusSettings'
import Account from './pages/Account'
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
              <Route path="/spam-settings" element={<SpamSettings />} />
              <Route path="/virus-settings" element={<VirusSettings />} />
              <Route path="/system" element={<System />} />
              <Route path="/audit-log" element={<AuditLog />} />
              <Route path="/account" element={<Account />} />
            </Route>
          </Route>
        </Routes>
      </BrowserRouter>
    </AuthProvider>
  )
}

export default App
