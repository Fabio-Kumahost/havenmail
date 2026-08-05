import { NavLink, Outlet, useNavigate } from 'react-router-dom'
import { useAuth } from '../AuthContext'

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
          Dashboard
        </NavLink>
        <NavLink to="/domains">Domains</NavLink>
        <NavLink to="/system">System</NavLink>
        <NavLink to="/audit-log">Audit-Log</NavLink>
        <button className="logout" onClick={onLogout}>
          Abmelden
        </button>
      </nav>
      <main className="content">
        <Outlet />
      </main>
    </div>
  )
}
