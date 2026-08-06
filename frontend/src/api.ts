/**
 * Havenmail Admin-Oberfläche — API-Client.
 *
 * Tokens liegen in sessionStorage (nicht localStorage): übersteht einen
 * Seiten-Reload, wird aber beim Schließen des Tabs/Browsers gelöscht.
 * Ursprünglich rein In-Memory gehalten ("um XSS-Exfiltration zu
 * erschweren") — das bedeutete aber, dass jeder Reload sofort ausgeloggt
 * hat, da die Modul-Variablen beim Neuladen der Seite verloren gehen und
 * RequireAuth ohne Tokens sofort zu /login umleitet. sessionStorage ist
 * derselbe Kompromiss, den die meisten SPAs eingehen (XSS könnte die
 * Tokens theoretisch auslesen, dasselbe Risiko wie bei localStorage) —
 * ein echtes HttpOnly-Cookie für den Refresh-Token wäre sicherer, bräuchte
 * aber Backend-/Reverse-Proxy-Änderungen und ist hier bewusst nicht
 * mitgemacht worden, um den Scope dieses Fixes klein zu halten.
 */

const API_BASE = import.meta.env.VITE_HAVENMAIL_API_URL ?? 'http://127.0.0.1:8080'

const ACCESS_TOKEN_KEY = 'havenmail_access_token'
const REFRESH_TOKEN_KEY = 'havenmail_refresh_token'

let accessToken: string | null = sessionStorage.getItem(ACCESS_TOKEN_KEY)
let refreshToken: string | null = sessionStorage.getItem(REFRESH_TOKEN_KEY)

export function setTokens(access: string, refresh: string) {
  accessToken = access
  refreshToken = refresh
  sessionStorage.setItem(ACCESS_TOKEN_KEY, access)
  sessionStorage.setItem(REFRESH_TOKEN_KEY, refresh)
}

export function clearTokens() {
  accessToken = null
  refreshToken = null
  sessionStorage.removeItem(ACCESS_TOKEN_KEY)
  sessionStorage.removeItem(REFRESH_TOKEN_KEY)
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

export interface UserStorage {
  id: string
  bytes: number | null
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
  mail_sent: number | null
  mail_received: number | null
}

export interface QueueRecipient {
  address: string
  delay_reason: string | null
}

export interface QueueEntry {
  queue_id: string
  queue_name: string
  arrival_time: string
  message_size: number
  sender: string
  recipients: QueueRecipient[]
}

export interface JailStatus {
  name: string
  banned: string[]
}

export interface Fail2banStatus {
  updated_at: string
  jails: JailStatus[]
}

export interface TotpRequired {
  totp_required: true
}

export type LoginResult = TokenResponse | TotpRequired

export function isTotpRequired(result: LoginResult): result is TotpRequired {
  return 'totp_required' in result
}

export interface TotpStatus {
  enabled: boolean
}

export interface TotpEnrollment {
  secret: string
  otpauth_uri: string
}

export interface BackupHistoryEntry {
  filename: string
  size_bytes: number
  created_at: string
}

export interface BackupLastRun {
  status: 'success' | 'failed'
  ran_at: string
  archive?: string
  error?: string
}

export interface BackupStatus {
  last_run: BackupLastRun | null
  history: BackupHistoryEntry[]
}

export const api = {
  login: (email: string, password: string, totp_code?: string) =>
    request<LoginResult>('POST', '/api/v1/auth/login', { email, password, totp_code }),
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
    storage: (domainId: string) =>
      request<UserStorage[]>('GET', `/api/v1/domains/${domainId}/users/storage`),
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
  totp: {
    status: () => request<TotpStatus>('GET', '/api/v1/users/me/totp'),
    enroll: () => request<TotpEnrollment>('POST', '/api/v1/users/me/totp/enroll'),
    confirm: (secret: string, code: string) =>
      request<{ status: string }>('POST', '/api/v1/users/me/totp/confirm', { secret, code }),
    disable: (password: string) =>
      request<{ status: string }>('POST', '/api/v1/users/me/totp/disable', { password }),
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
  mailQueue: {
    list: () => request<QueueEntry[]>('GET', '/api/v1/system/mail-queue'),
    deleteOne: (queueId: string) =>
      request<{ status: string }>('DELETE', `/api/v1/system/mail-queue/${queueId}`),
    deleteAll: () => request<{ status: string }>('DELETE', '/api/v1/system/mail-queue'),
  },
  fail2ban: {
    status: () => request<Fail2banStatus>('GET', '/api/v1/system/fail2ban'),
    unban: (jail: string, ip: string) =>
      request<{ status: string }>('POST', '/api/v1/system/fail2ban/unban', { jail, ip }),
  },
  backup: {
    status: () => request<BackupStatus>('GET', '/api/v1/system/backup'),
    trigger: () => request<{ status: string }>('POST', '/api/v1/system/backup/trigger'),
  },
}

export { ApiError }
