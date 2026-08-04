/**
 * Havenmail Admin-Oberfläche.
 *
 * STATUS (M4): Login, Dashboard (nur Health-Status), Domain-Verwaltung,
 * Benutzer-/Alias-CRUD, DNS-Einrichtungsassistent (kopierbare Einträge +
 * Live-Prüfung), DKIM-Schlüsselerzeugung. Weitere in der Aufgabenstellung
 * genannte Bereiche (Quotas, Warteschlangen, Zustellfehler, Spam-/
 * Virenereignisse, TLS-Zertifikate, System-/Audit-Protokolle, Backup-Status,
 * Updates, Dienste/Ressourcen) sind noch nicht umgesetzt — sie benötigen
 * die installierten Mail-Engines (M5) bzw. weitere Backend-Endpunkte, die
 * noch nicht existieren. Siehe CHANGELOG.md für den genauen Stand.
 */
import { BrowserRouter, Routes, Route } from 'react-router-dom'
import { AuthProvider } from './AuthContext'
import RequireAuth from './pages/RequireAuth'
import Layout from './pages/Layout'
import Login from './pages/Login'
import Dashboard from './pages/Dashboard'
import Domains from './pages/Domains'
import DomainDetail from './pages/DomainDetail'
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
            </Route>
          </Route>
        </Routes>
      </BrowserRouter>
    </AuthProvider>
  )
}

export default App
