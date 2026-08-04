import { Navigate, Outlet } from 'react-router-dom'
import { useAuth } from '../AuthContext'

export default function RequireAuth() {
  const { loggedIn } = useAuth()
  if (!loggedIn) return <Navigate to="/login" replace />
  return <Outlet />
}
