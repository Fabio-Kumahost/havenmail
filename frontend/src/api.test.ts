import { describe, expect, it, vi, beforeEach } from 'vitest'
import { api, ApiError, clearTokens } from './api'

describe('api', () => {
  beforeEach(() => {
    clearTokens()
  })

  it('wirft ApiError mit Server-Fehlermeldung bei fehlgeschlagenem Login', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue({
        ok: false,
        status: 401,
        json: async () => ({ error: 'nicht authentifiziert' }),
      } as Response),
    )
    await expect(api.login('a@b.test', 'wrong')).rejects.toThrow('nicht authentifiziert')
  })

  it('propagiert ApiError-Instanz mit korrektem Status', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue({
        ok: false,
        status: 403,
        json: async () => ({ error: 'keine Berechtigung' }),
      } as Response),
    )
    try {
      await api.domains.list()
      expect.unreachable()
    } catch (err) {
      expect(err).toBeInstanceOf(ApiError)
      expect((err as ApiError).status).toBe(403)
    }
  })
})
