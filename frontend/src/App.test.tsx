import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi, beforeEach } from 'vitest'
import App from './App'

describe('App', () => {
  beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: true, json: async () => ({}) } as Response))
  })

  it('leitet nicht angemeldete Nutzer zum Login um', async () => {
    render(<App />)
    expect(await screen.findByRole('heading', { name: 'Havenmail Admin' })).toBeInTheDocument()
  })
})
