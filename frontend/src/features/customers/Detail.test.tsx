import { fireEvent, render, screen } from '@testing-library/react'
import { MantineProvider } from '@mantine/core'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { createMemoryHistory, createRootRoute, createRoute, createRouter, RouterProvider } from '@tanstack/react-router'
import { afterEach, describe, expect, it, vi } from 'vitest'

import '../../lib/i18n'
import { AuthContext } from '../../lib/auth/context'
import { customersKeys } from './api'
import { CustomerDetail } from './Detail'
import type { Customer } from './types'

const customer: Customer = {
  id: 'c1',
  kind: 0,
  name: 'Adamant Print GmbH',
  legal_name: null,
  edrpou: null,
  tax_id: null,
  vat_ipn: null,
  status: 1,
  tags: [],
  industry: null,
  source: null,
  website: null,
  contacts: [],
  legal_address: null,
  delivery_address: null,
  payment_terms_days: 0,
  credit_limit: null,
  default_currency: 'EUR',
  default_discount_bp: 0,
  iban: null,
  bank_name: null,
  notes: null,
  created_at: '2026-01-01T00:00:00Z',
  updated_at: '2026-01-01T00:00:00Z',
  version: 1,
}

function json(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), { status })
}

function renderDetail() {
  const queryClient = new QueryClient()
  const rootRoute = createRootRoute()
  const detailRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: '/customers/$id',
    component: CustomerDetail,
  })
  const router = createRouter({
    routeTree: rootRoute.addChildren([detailRoute]),
    history: createMemoryHistory({ initialEntries: ['/customers/c1'] }),
  })

  const { container } = render(
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

  return { queryClient, container }
}

describe('CustomerDetail optimistic concurrency', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('sends the If-Match version snapshotted at edit-start, not a WS-refreshed one', async () => {
    let putHeaders: Record<string, string> | undefined
    vi.stubGlobal(
      'fetch',
      vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        const url = String(input)
        if (url.includes('/api/dictionaries/customer-statuses')) {
          return json({
            items: [{ id: 1, key: 'active', sort: 1, color: 'green', can_order: true, allowed_targets: [], labels: { en: 'Active' } }],
          })
        }
        if (url.includes('/api/dictionaries/order-statuses')) {
          return json({ items: [] })
        }
        if (url.includes('/activity')) {
          return json({
            total_orders: 0,
            status_counts: [],
            orders_by_month: [],
            total_spend: { amount_minor: 0, currency: 'EUR' },
            orders_last_30_days: 0,
            last_order_at: null,
          })
        }
        if (url.includes('/api/customers/c1')) {
          if (init?.method === 'PUT') {
            putHeaders = init.headers as Record<string, string>
            return json({ ...customer, name: 'My Edit AG', version: 2 })
          }
          return json(customer)
        }
        return json({})
      }),
    )

    const { queryClient, container } = renderDetail()

    expect(await screen.findByText('Adamant Print GmbH')).toBeInTheDocument()

    // Enter edit mode — the OCC token is frozen at version 1 here.
    fireEvent.click(screen.getByRole('button', { name: 'Edit' }))

    // A concurrent write by another user arrives over the live-updates socket
    // and refreshes the detail cache to the new server version.
    queryClient.setQueryData<Customer>(customersKeys.detail('c1'), {
      ...customer,
      name: 'Concurrent Rename',
      version: 2,
    })

    const nameField = container.querySelector<HTMLInputElement>('[data-path="name"]')!
    fireEvent.change(nameField, { target: { value: 'My Edit AG' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await vi.waitFor(() => expect(putHeaders).toBeDefined())
    // The snapshot (1) must win over the cache's refreshed value (2), so the
    // server can reject the stale write instead of silently clobbering.
    expect(putHeaders?.['if-match']).toBe('1')
  })
})
