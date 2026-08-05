import { NavLink, Outlet, useNavigate } from 'react-router-dom'
import { useAuth } from '../AuthContext'

const ICONS = {
  dashboard: (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
      <rect x="3" y="3" width="7" height="9" rx="1.5" />
      <rect x="14" y="3" width="7" height="5" rx="1.5" />
      <rect x="14" y="12" width="7" height="9" rx="1.5" />
      <rect x="3" y="16" width="7" height="5" rx="1.5" />
    </svg>
  ),
  domains: (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
      <circle cx="12" cy="12" r="9" />
      <path d="M3 12h18M12 3c2.5 2.6 3.8 5.9 3.8 9s-1.3 6.4-3.8 9c-2.5-2.6-3.8-5.9-3.8-9s1.3-6.4 3.8-9Z" />
    </svg>
  ),
  system: (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
      <rect x="4" y="4" width="16" height="7" rx="1.5" />
      <rect x="4" y="13" width="16" height="7" rx="1.5" />
      <circle cx="8" cy="7.5" r="0.6" fill="currentColor" stroke="none" />
      <circle cx="8" cy="16.5" r="0.6" fill="currentColor" stroke="none" />
    </svg>
  ),
  audit: (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
      <path d="M9 4h9a1 1 0 0 1 1 1v14a1 1 0 0 1-1 1H7a1 1 0 0 1-1-1V8Z" />
      <path d="M9 4v4H5" />
      <path d="M9 13h6M9 17h4" />
    </svg>
  ),
  spam: (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
      <path d="M4 4h16v4l-8 5-8-5V4Z" />
      <path d="M4 8v12h16V8" />
      <path d="m14.5 15.5 4 4M18.5 15.5l-4 4" />
    </svg>
  ),
  virus: (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
      <path d="M12 2c2.5 2.6 3.8 5.9 3.8 10s-1.3 7.4-3.8 10c-2.5-2.6-3.8-5.9-3.8-10s1.3-7.4 3.8-10Z" />
      <path d="M2 12c2.6-2.5 5.9-3.8 10-3.8s7.4 1.3 10 3.8c-2.6 2.5-5.9 3.8-10 3.8S4.6 14.5 2 12Z" />
    </svg>
  ),
  account: (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
      <circle cx="12" cy="8" r="3.5" />
      <path d="M4.5 20c1.4-3.6 4.4-5.5 7.5-5.5s6.1 1.9 7.5 5.5" />
    </svg>
  ),
  logout: (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
      <path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4" />
      <path d="M16 17l5-5-5-5M21 12H9" />
    </svg>
  ),
}

export default function Layout() {
  const { logout } = useAuth()
  const navigate = useNavigate()

  function onLogout() {
    logout()
    navigate('/login')
  }

  return (
    <div className="app-shell">
      <nav className="sidebar" aria-label="Hauptnavigation">
        <div className="brand">Havenmail</div>
        <NavLink to="/" end>
          {ICONS.dashboard}
          <span>Dashboard</span>
        </NavLink>
        <NavLink to="/domains">
          {ICONS.domains}
          <span>Domains</span>
        </NavLink>
        <NavLink to="/spam-settings">
          {ICONS.spam}
          <span>Spam-Schutz</span>
        </NavLink>
        <NavLink to="/virus-settings">
          {ICONS.virus}
          <span>Virenschutz</span>
        </NavLink>
        <NavLink to="/system">
          {ICONS.system}
          <span>System</span>
        </NavLink>
        <NavLink to="/audit-log">
          {ICONS.audit}
          <span>Audit-Log</span>
        </NavLink>
        <NavLink to="/account" className="sidebar-account">
          {ICONS.account}
          <span>Mein Konto</span>
        </NavLink>
        <button className="logout" onClick={onLogout}>
          {ICONS.logout}
          <span>Abmelden</span>
        </button>
      </nav>
      <main className="content">
        <Outlet />
      </main>
    </div>
  )
}
