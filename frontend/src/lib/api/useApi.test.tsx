import type { ReactNode } from 'react'
import { renderHook } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { AuthContext } from '../auth/context'
import type { AuthContextValue } from '../auth/context'
import { useApi } from './useApi'

function wrapper(auth: AuthContextValue) {
  return ({ children }: { children: ReactNode }) => (
    <AuthContext.Provider value={auth}>{children}</AuthContext.Provider>
  )
}

function authValue(overrides: Partial<AuthContextValue> = {}): AuthContextValue {
  return {
    mode: 'dev',
    orgId: 'org_dev1',
    getToken: async () => 'tok',
    signOut: vi.fn(),
    ...overrides,
  }
}

describe('useApi', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('signs out on a 401 so the dead token is not reused', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => new Response(JSON.stringify({ error: { code: 'unauthorized', message: 'no' } }), { status: 401 })),
    )
    const auth = authValue()
    const { result } = renderHook(() => useApi(), { wrapper: wrapper(auth) })

    await expect(result.current('/api/me')).rejects.toMatchObject({ status: 401 })
    expect(auth.signOut).toHaveBeenCalledTimes(1)
  })

  it('does not sign out on non-401 failures', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => new Response(JSON.stringify({ error: { code: 'server', message: 'boom' } }), { status: 500 })),
    )
    const auth = authValue()
    const { result } = renderHook(() => useApi(), { wrapper: wrapper(auth) })

    await expect(result.current('/api/me')).rejects.toMatchObject({ status: 500 })
    expect(auth.signOut).not.toHaveBeenCalled()
  })
})
