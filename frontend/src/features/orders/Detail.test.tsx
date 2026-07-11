import { fireEvent, render, screen } from '@testing-library/react'
import { MantineProvider } from '@mantine/core'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { createMemoryHistory, createRootRoute, createRoute, createRouter, RouterProvider } from '@tanstack/react-router'
import { afterEach, describe, expect, it, vi } from 'vitest'

import '../../lib/i18n'
import { AuthContext } from '../../lib/auth/context'
import { ordersKeys } from './api'
import { OrderDetail } from './Detail'
import type { Order } from './types'

const order: Order = {
  id: 'o1',
  number: 'ORD-2026-0001',
  customer_id: 'c1',
  customer_name: 'Adamant Print GmbH',
  status: 'draft',
  currency: 'EUR',
  line_items: [{ description: 'Boxes', quantity: 10, unit_price: { amount_minor: 250, currency: 'EUR' } }],
  total: { amount_minor: 2500, currency: 'EUR' },
  notes: null,
  created_at: '2026-01-01T00:00:00Z',
  updated_at: '2026-01-01T00:00:00Z',
}

function renderDetail() {
  const queryClient = new QueryClient()
  const rootRoute = createRootRoute()
  const detailRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: '/orders/$id',
    component: OrderDetail,
  })
  const router = createRouter({
    routeTree: rootRoute.addChildren([detailRoute]),
    history: createMemoryHistory({ initialEntries: ['/orders/o1'] }),
  })

  render(
    <MantineProvider>
      <QueryClientProvider client={queryClient}>
        <AuthContext.Provider
          value={{ mode: 'dev', orgId: 'org_test', getToken: async () => 'test-token', signOut: () => {} }}
        >
          <RouterProvider router={router} />
        </AuthContext.Provider>
      </QueryClientProvider>
    </MantineProvider>,
  )

  return { queryClient }
}

describe('OrderDetail status transition', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('applies the status optimistically and rolls back when the server rejects it', async () => {
    let respondToStatus!: (response: Response) => void
    vi.stubGlobal(
      'fetch',
      vi.fn(async (input: RequestInfo | URL) => {
        if (String(input).includes('/status')) {
          return new Promise<Response>((resolve) => {
            respondToStatus = resolve
          })
        }
        return new Response(JSON.stringify(order), { status: 200 })
      }),
    )
    const { queryClient } = renderDetail()

    expect(await screen.findByText('Draft')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Mark as Confirmed' }))

    expect(await screen.findByText('Confirmed')).toBeInTheDocument()
    expect(queryClient.getQueryData<Order>(ordersKeys.detail('o1'))?.status).toBe('confirmed')

    respondToStatus(
      new Response(
        JSON.stringify({
          error: {
            code: 'order_status_transition',
            message: 'cannot transition order from Draft to InProduction',
            details: { from: 'draft', to: 'in_production' },
          },
        }),
        { status: 409 },
      ),
    )

    expect(await screen.findByText('Draft')).toBeInTheDocument()
    expect(await screen.findByText('Cannot change the order status from Draft to In production.')).toBeInTheDocument()
    expect(queryClient.getQueryData<Order>(ordersKeys.detail('o1'))?.status).toBe('draft')
  })
})
