import { useEffect, useState } from 'react'
import { NavLink, Outlet, useNavigate } from 'react-router-dom'
import { useAuth } from '../AuthContext'
import { useBranding } from '../BrandingContext'
import { api, type SearchResult } from '../api'

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
  overview: (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
      <rect x="3" y="3" width="8" height="8" rx="1.5" />
      <rect x="13" y="3" width="8" height="8" rx="1.5" />
      <rect x="3" y="13" width="8" height="8" rx="1.5" />
      <rect x="13" y="13" width="8" height="8" rx="1.5" />
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
  branding: (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
      <path d="M4 4h9l7 7-9 9-7-7V4Z" />
      <circle cx="9" cy="9" r="1.3" fill="currentColor" stroke="none" />
    </svg>
  ),
  shield: (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
      <path d="M12 3 5 6v5c0 5 3 8.5 7 10 4-1.5 7-5 7-10V6l-7-3Z" />
      <path d="m9.5 12 1.8 1.8L15 10" />
    </svg>
  ),
  backup: (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
      <path d="M12 3v12" />
      <path d="m7 10 5 5 5-5" />
      <path d="M5 15v3a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2v-3" />
    </svg>
  ),
  logout: (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
      <path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4" />
      <path d="M16 17l5-5-5-5M21 12H9" />
    </svg>
  ),
  menu: (
    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
      <path d="M4 7h16M4 12h16M4 17h16" />
    </svg>
  ),
  close: (
    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
      <path d="M6 6l12 12M18 6 6 18" />
    </svg>
  ),
}

export default function Layout() {
  const { logout } = useAuth()
  const { branding } = useBranding()
  const navigate = useNavigate()
  // Sidebar ist ab der mobilen Breakpoint (siehe App.css, 860px) ein
  // ausblendbares Slide-in-Menü statt einer permanent sichtbaren Spalte
  // — auf schmalen Bildschirmen sonst zu viele Nav-Punkte + Suchfeld für
  // eine dauerhaft sichtbare Leiste (führte zu horizontalem Scrollen der
  // gesamten Navigation, vom Nutzer als "verzerrt" gemeldet).
  const [mobileNavOpen, setMobileNavOpen] = useState(false)

  function onLogout() {
    logout()
    navigate('/login')
  }

  function closeMobileNav() {
    setMobileNavOpen(false)
  }

  return (
    <div className="app-shell">
      <div className="mobile-topbar">
        <button
          type="button"
          className="mobile-nav-toggle"
          aria-label={mobileNavOpen ? 'Menü schließen' : 'Menü öffnen'}
          aria-expanded={mobileNavOpen}
          onClick={() => setMobileNavOpen((open) => !open)}
        >
          {mobileNavOpen ? ICONS.close : ICONS.menu}
        </button>
        <div className="brand">
          {branding.logo_url && (
            <img src={branding.logo_url} alt="" style={{ height: '1.4rem', width: 'auto' }} />
          )}
          <span>{branding.product_name}</span>
        </div>
      </div>
      {mobileNavOpen && (
        <div className="mobile-nav-backdrop" onClick={closeMobileNav} aria-hidden="true" />
      )}
      <nav
        className={`sidebar${mobileNavOpen ? ' sidebar-open' : ''}`}
        aria-label="Hauptnavigation"
        onClick={(e) => {
          // Schließt das Slide-in-Menü, sobald ein Nav-Link/Button darin
          // geklickt wird (Klicks auf leeren Zwischenraum ignorieren wir
          // bewusst nicht extra — harmlos, wenn das Menü dabei mitschließt).
          if ((e.target as HTMLElement).closest('a, button')) closeMobileNav()
        }}
      >
        <div className="brand" style={{ display: 'flex', alignItems: 'center', gap: '0.5rem' }}>
          {branding.logo_url && (
            <img src={branding.logo_url} alt="" style={{ height: '1.5rem', width: 'auto' }} />
          )}
          <span>{branding.product_name}</span>
        </div>
        <GlobalSearch />
        <NavLink to="/" end>
          {ICONS.dashboard}
          <span>Dashboard</span>
        </NavLink>
        <NavLink to="/overview">
          {ICONS.overview}
          <span>Übersicht</span>
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
        <NavLink to="/fail2ban">
          {ICONS.shield}
          <span>Fail2Ban</span>
        </NavLink>
        <NavLink to="/backup">
          {ICONS.backup}
          <span>Backup</span>
        </NavLink>
        <NavLink to="/system">
          {ICONS.system}
          <span>System</span>
        </NavLink>
        <NavLink to="/audit-log">
          {ICONS.audit}
          <span>Audit-Log</span>
        </NavLink>
        <NavLink to="/branding">
          {ICONS.branding}
          <span>Branding</span>
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

/**
 * Domänenübergreifende Live-Suche (debounced) in der Sidebar — man muss
 * nicht mehr vorher wissen, in welcher Domain ein Postfach liegt. Ein
 * Klick auf ein Ergebnis navigiert zur Domain-Detail-Seite (auch bei
 * einem Postfach-Treffer — es gibt noch keine eigene Postfach-Detailseite
 * zum direkt Hinspringen).
 */
function GlobalSearch() {
  const navigate = useNavigate()
  const [query, setQuery] = useState('')
  const [results, setResults] = useState<SearchResult[]>([])
  const [open, setOpen] = useState(false)
  const [searching, setSearching] = useState(false)

  useEffect(() => {
    if (query.trim().length < 2) {
      setResults([])
      return
    }
    setSearching(true)
    const timer = setTimeout(() => {
      api
        .search(query.trim())
        .then((r) => {
          setResults(r)
          setOpen(true)
        })
        .catch(() => setResults([]))
        .finally(() => setSearching(false))
    }, 250)
    return () => clearTimeout(timer)
  }, [query])

  function onSelect(result: SearchResult) {
    setOpen(false)
    setQuery('')
    navigate(`/domains/${result.domain_id}`)
  }

  return (
    <div style={{ position: 'relative', padding: '0 0.75rem', marginBottom: '0.5rem' }}>
      <input
        type="search"
        placeholder="Postfach oder Domain suchen…"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        onFocus={() => results.length > 0 && setOpen(true)}
        onBlur={() => setTimeout(() => setOpen(false), 150)}
        style={{ width: '100%' }}
      />
      {open && (
        <div
          className="card"
          style={{
            position: 'absolute',
            top: '100%',
            left: '0.75rem',
            right: '0.75rem',
            zIndex: 20,
            maxHeight: '20rem',
            overflowY: 'auto',
            padding: '0.5rem',
          }}
        >
          {searching && <p className="muted" style={{ margin: 0 }}>Suche…</p>}
          {!searching && results.length === 0 && (
            <p className="muted" style={{ margin: 0 }}>Keine Treffer.</p>
          )}
          {!searching &&
            results.map((r, i) => (
              <button
                key={i}
                type="button"
                onClick={() => onSelect(r)}
                style={{
                  display: 'block',
                  width: '100%',
                  textAlign: 'left',
                  background: 'none',
                  border: 'none',
                  padding: '0.4rem 0.25rem',
                }}
              >
                {r.kind === 'domain' ? (
                  <>
                    <strong>{r.domain_name}</strong> <span className="muted">Domain</span>
                  </>
                ) : (
                  <>
                    {r.local_part}@{r.domain_name} <span className="muted">Postfach</span>
                  </>
                )}
              </button>
            ))}
        </div>
      )}
    </div>
  )
}
