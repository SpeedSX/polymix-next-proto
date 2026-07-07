import { fireEvent, render, screen } from '@testing-library/react'
import { MantineProvider } from '@mantine/core'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { RouterProvider } from '@tanstack/react-router'
import { afterEach, describe, expect, it, vi } from 'vitest'

import '../lib/i18n'
import { AuthProvider } from '../lib/auth'
import { router } from './routes'

describe('app shell', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
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

    expect(await screen.findAllByText('PolyMix Next')).not.toHaveLength(0)
  })
})
