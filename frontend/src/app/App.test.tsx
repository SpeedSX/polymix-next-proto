import { fireEvent, render, screen } from '@testing-library/react'
import { MantineProvider } from '@mantine/core'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { RouterProvider } from '@tanstack/react-router'
import { afterEach, describe, expect, it, vi } from 'vitest'

import '../lib/i18n'
import { AuthProvider } from '../lib/auth'
import { DEV_SESSION_STORAGE_KEY } from '../lib/auth/DevAuthProvider'
import { router } from './routes'

describe('app shell', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
    localStorage.removeItem(DEV_SESSION_STORAGE_KEY)
  })

  it('signs in via the dev form and renders the app shell', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => new Response(JSON.stringify({ token: 'test-token' }), { status: 200 })),
    )

    const queryClient = new QueryClient()

    render(
      <MantineProvider>
        <QueryClientProvider client={queryClient}>
          <AuthProvider>
            <RouterProvider router={router} />
          </AuthProvider>
        </QueryClientProvider>
      </MantineProvider>,
    )

    fireEvent.click(await screen.findByRole('button', { name: 'Sign in' }))

    expect(await screen.findAllByText('Customers')).not.toHaveLength(0)
    expect(localStorage.getItem(DEV_SESSION_STORAGE_KEY)).toContain('test-token')
  })

  it('restores a stored dev session across remounts', async () => {
    localStorage.setItem(
      DEV_SESSION_STORAGE_KEY,
      JSON.stringify({ token: 'stored-token', orgId: 'org_dev1' }),
    )

    const queryClient = new QueryClient()

    render(
      <MantineProvider>
        <QueryClientProvider client={queryClient}>
          <AuthProvider>
            <RouterProvider router={router} />
          </AuthProvider>
        </QueryClientProvider>
      </MantineProvider>,
    )

    expect(await screen.findAllByText('Customers')).not.toHaveLength(0)
    expect(screen.queryByRole('button', { name: 'Sign in' })).toBeNull()
  })
})
