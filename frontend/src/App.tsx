/**
 * Havenmail Admin-Oberfläche.
 *
 * STATUS: Login, Dashboard (Health-Status + Verlaufscharts), Domain-
 * Verwaltung, Benutzer-/Alias-CRUD, DNS-Einrichtungsassistent (kopierbare
 * Einträge + Live-Prüfung), DKIM-Schlüsselerzeugung, System-Seite
 * (Dienststatus der Mail-Engines), Audit-Log-Ansicht (domänen-gescoped für
 * domain_admin), Spam-/Virenschutz-Einstellungen, Selbstbedienungs-
 * Passwortänderung, 2FA, Sitzungsverwaltung, API-Keys, Fail2Ban, Backup,
 * CSV-Import/Export, RBL-Monitoring, domänenübergreifende Übersicht.
 * Weiterhin offen: Zustellfehler-Auswertung. Siehe CHANGELOG.md für den
 * genauen Stand.
 */
import { BrowserRouter, Routes, Route } from 'react-router-dom'
import { AuthProvider } from './AuthContext'
import { BrandingProvider } from './BrandingContext'
import RequireAuth from './pages/RequireAuth'
import Layout from './pages/Layout'
import Login from './pages/Login'
import Dashboard from './pages/Dashboard'
import Overview from './pages/Overview'
import Domains from './pages/Domains'
import DomainDetail from './pages/DomainDetail'
import System from './pages/System'
import AuditLog from './pages/AuditLog'
import SpamSettings from './pages/SpamSettings'
import VirusSettings from './pages/VirusSettings'
import Fail2Ban from './pages/Fail2Ban'
import Backup from './pages/Backup'
import Branding from './pages/Branding'
import Account from './pages/Account'
import './App.css'

function App() {
  return (
    <BrandingProvider>
      <AuthProvider>
        <BrowserRouter>
          <Routes>
            <Route path="/login" element={<Login />} />
            <Route element={<RequireAuth />}>
              <Route element={<Layout />}>
                <Route path="/" element={<Dashboard />} />
                <Route path="/overview" element={<Overview />} />
                <Route path="/domains" element={<Domains />} />
                <Route path="/domains/:domainId" element={<DomainDetail />} />
                <Route path="/spam-settings" element={<SpamSettings />} />
                <Route path="/virus-settings" element={<VirusSettings />} />
                <Route path="/fail2ban" element={<Fail2Ban />} />
                <Route path="/backup" element={<Backup />} />
                <Route path="/branding" element={<Branding />} />
                <Route path="/system" element={<System />} />
                <Route path="/audit-log" element={<AuditLog />} />
                <Route path="/account" element={<Account />} />
              </Route>
            </Route>
          </Routes>
        </BrowserRouter>
      </AuthProvider>
    </BrandingProvider>
  )
}

export default App
