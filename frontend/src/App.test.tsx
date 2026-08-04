import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi, beforeEach } from 'vitest'
import App from './App'

describe('App', () => {
  beforeEach(() => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue({ ok: true } as Response),
    )
  })

  it('rendert den Havenmail-Titel', () => {
    render(<App />)
    expect(screen.getByRole('heading', { name: 'Havenmail' })).toBeInTheDocument()
  })

  it('zeigt zunächst den Prüfstatus der API an', () => {
    render(<App />)
    expect(screen.getByText('checking')).toBeInTheDocument()
  })
})
