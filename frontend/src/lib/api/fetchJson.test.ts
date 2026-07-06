import { afterEach, describe, expect, it, vi } from 'vitest'

import { fetchJson } from './fetchJson'

describe('fetchJson', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('injects the bearer token and returns parsed JSON', async () => {
    const fetchMock = vi.fn<(url: string, init?: RequestInit) => Promise<Response>>(
      async () => new Response(JSON.stringify({ ok: true }), { status: 200 }),
    )
    vi.stubGlobal('fetch', fetchMock)

    const result = await fetchJson<{ ok: boolean }>('/api/customers', {
      getToken: async () => 'tok',
    })

    expect(result).toEqual({ ok: true })
    const init = fetchMock.mock.calls[0][1]
    expect((init?.headers as Record<string, string>).authorization).toBe('Bearer tok')
  })

  it('throws a typed ApiError parsed from the error envelope', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(
        async () =>
          new Response(
            JSON.stringify({
              error: { code: 'validation_failed', message: 'bad', details: { name: 'required' } },
            }),
            { status: 422 },
          ),
      ),
    )

    await expect(fetchJson('/api/customers', { getToken: async () => null })).rejects.toMatchObject({
      status: 422,
      code: 'validation_failed',
      details: { name: 'required' },
    })
  })
})
