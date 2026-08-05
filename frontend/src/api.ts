/**
 * Havenmail Admin-Oberfläche — API-Client.
 *
 * STATUS (M4): dünner fetch-Wrapper gegen die REST-API aus M2/M3
 * (backend/crates/api). Access-Token wird im Speicher gehalten (nicht in
 * localStorage, um XSS-Exfiltration zu erschweren); Refresh-Token liegt in
 * einem HttpOnly-Cookie — Details folgen mit dem Installer/Reverse-Proxy-
 * Setup in M5. Für die aktuelle Entwicklungsphase wird der Refresh-Token
 * ebenfalls im Speicher gehalten.
 */

const API_BASE = import.meta.env.VITE_HAVENMAIL_API_URL ?? 'http://127.0.0.1:8080'

let accessToken: string | null = null
let refreshToken: string | null = null

export function setTokens(access: string, refresh: string) {
  accessToken = access
  refreshToken = refresh
}

export function clearTokens() {
  accessToken = null
  refreshToken = null
}

export function isAuthenticated() {
  return accessToken !== null
}

class ApiError extends Error {
  status: number
  constructor(status: number, message: string) {
    super(message)
    this.status = status
  }
}

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const headers: Record<string, string> = { 'content-type': 'application/json' }
  if (accessToken) headers.authorization = `Bearer ${accessToken}`

  let res = await fetch(`${API_BASE}${path}`, {
    method,
    headers,
    body: body !== undefined ? JSON.stringify(body) : undefined,
  })

  if (res.status === 401 && refreshToken && path !== '/api/v1/auth/refresh') {
    const refreshed = await tryRefresh()
    if (refreshed) {
      headers.authorization = `Bearer ${accessToken}`
      res = await fetch(`${API_BASE}${path}`, {
        method,
        headers,
        body: body !== undefined ? JSON.stringify(body) : undefined,
      })
    }
  }

  const data = await res.json().catch(() => null)
  if (!res.ok) {
    throw new ApiError(res.status, data?.error ?? `HTTP ${res.status}`)
  }
  return data as T
}

async function tryRefresh(): Promise<boolean> {
  try {
    const res = await fetch(`${API_BASE}/api/v1/auth/refresh`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ refresh_token: refreshToken }),
    })
    if (!res.ok) return false
    const data = await res.json()
    setTokens(data.access_token, data.refresh_token)
    return true
  } catch {
    return false
  }
}

export interface TokenResponse {
  access_token: string
  refresh_token: string
}

export interface Domain {
  id: string
  name: string
  is_active: boolean
  catch_all_enabled: boolean
  catch_all_target: string | null
  quota_bytes: number | null
  created_at: string
}

export interface User {
  id: string
  domain_id: string
  local_part: string
  role: string
  quota_bytes: number | null
  is_active: boolean
  created_at: string
}

export interface Alias {
  id: string
  domain_id: string
  source: string
  destinations: string[]
  is_active: boolean
}

export interface DnsEntry {
  record_type: string
  name: string
  value: string
}

export interface DnsRecommendations {
  mx: DnsEntry
  spf: DnsEntry
  dkim: DnsEntry | null
  dmarc: DnsEntry
}

export interface DnsCheckResult {
  record_type: string
  expected: string
  actual: string | null
  status: 'ok' | 'missing' | 'mismatch'
}

export interface ServiceStatus {
  unit: string
  active: boolean
  detail: string
}

export interface TlsStatus {
  expires_at: string
  days_remaining: number | null
}

export interface SystemStatus {
  database: boolean
  services: ServiceStatus[]
  tls: TlsStatus | null
}

export interface AuditLogEntry {
  id: string
  actor_id: string | null
  action: string
  target: string
  domain_id: string | null
  before: unknown
  after: unknown
  ip: string | null
  created_at: string
}

export interface SecuritySettings {
  spam_greylist_score: number
  spam_add_header_score: number
  spam_reject_score: number
  dmarc_enabled: boolean
  ratelimit_enabled: boolean
  ratelimit_per_hour: number
  ratelimit_burst: number
  antivirus_enabled: boolean
  antivirus_action: 'reject' | 'add_header' | 'no_action'
  antivirus_max_size_mb: number
  updated_at: string
}

export interface MetricsPoint {
  captured_at: string
  spam_delta: number | null
  ham_delta: number | null
  scanned_delta: number | null
  reject_delta: number | null
  virus_detected: number | null
  mail_queue_size: number | null
  disk_used_percent: number | null
}

export const api = {
  login: (email: string, password: string) =>
    request<TokenResponse>('POST', '/api/v1/auth/login', { email, password }),
  domains: {
    list: () => request<Domain[]>('GET', '/api/v1/domains'),
    create: (name: string) => request<Domain>('POST', '/api/v1/domains', { name }),
    get: (id: string) => request<Domain>('GET', `/api/v1/domains/${id}`),
    update: (id: string, patch: Partial<Domain>) =>
      request<Domain>('PATCH', `/api/v1/domains/${id}`, patch),
    delete: (id: string) => request<void>('DELETE', `/api/v1/domains/${id}`),
  },
  users: {
    list: (domainId: string) => request<User[]>('GET', `/api/v1/domains/${domainId}/users`),
    create: (domainId: string, local_part: string, password: string, role: string) =>
      request<User>('POST', `/api/v1/domains/${domainId}/users`, { local_part, password, role }),
    update: (id: string, patch: { is_active?: boolean; password?: string; quota_bytes?: number }) =>
      request<User>('PATCH', `/api/v1/users/${id}`, patch),
    delete: (id: string) => request<void>('DELETE', `/api/v1/users/${id}`),
  },
  aliases: {
    list: (domainId: string) => request<Alias[]>('GET', `/api/v1/domains/${domainId}/aliases`),
    create: (domainId: string, source: string, destinations: string[]) =>
      request<Alias>('POST', `/api/v1/domains/${domainId}/aliases`, { source, destinations }),
    delete: (id: string) => request<void>('DELETE', `/api/v1/aliases/${id}`),
  },
  dns: {
    recommendations: (domainId: string) =>
      request<DnsRecommendations>('GET', `/api/v1/domains/${domainId}/dns-recommendations`),
    generateDkim: (domainId: string) =>
      request<{ selector: string; dns_record_name: string; dns_record_value: string }>(
        'POST',
        `/api/v1/domains/${domainId}/dkim`,
      ),
    check: (domainId: string) =>
      request<{ results: DnsCheckResult[] }>('POST', `/api/v1/domains/${domainId}/dns-check`),
  },
  health: {
    ready: () => request<{ status: string; checks: Record<string, boolean> }>('GET', '/readyz'),
  },
  system: {
    status: () => request<SystemStatus>('GET', '/api/v1/system/status'),
  },
  auditLog: {
    list: (domainId?: string) =>
      request<AuditLogEntry[]>(
        'GET',
        `/api/v1/audit-log${domainId ? `?domain_id=${domainId}` : ''}`,
      ),
  },
  account: {
    changePassword: (current_password: string, new_password: string) =>
      request<User>('PATCH', '/api/v1/users/me/password', { current_password, new_password }),
  },
  securitySettings: {
    get: () => request<SecuritySettings>('GET', '/api/v1/system/security-settings'),
    updateSpam: (patch: Partial<SecuritySettings>) =>
      request<SecuritySettings>('PATCH', '/api/v1/system/spam-settings', patch),
    updateVirus: (patch: Partial<SecuritySettings>) =>
      request<SecuritySettings>('PATCH', '/api/v1/system/virus-settings', patch),
  },
  metrics: {
    range: (range: '7d' | '30d' = '7d') =>
      request<MetricsPoint[]>('GET', `/api/v1/system/metrics?range=${range}`),
  },
}

export { ApiError }
