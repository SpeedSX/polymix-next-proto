import { QueryClient } from '@tanstack/react-query'
import { describe, expect, it } from 'vitest'

import { customersKeys } from '../../features/customers/api'
import { invoicesKeys } from '../../features/invoices/api'
import { ordersKeys } from '../../features/orders/api'
import { applyChange, handleServerFrame } from './applyChange'
import type { ChangeFrame } from './types'

const listParams = { page: 1, limit: 25, sort: '-created_at' }

const entities = [
  { entity: 'customer', keys: customersKeys },
  { entity: 'order', keys: ordersKeys },
  { entity: 'invoice', keys: invoicesKeys },
] as const

function seededClient(keys: (typeof entities)[number]['keys']) {
  const queryClient = new QueryClient()
  queryClient.setQueryData(keys.list(listParams), { items: [], total: 0, page: 1, limit: 25 })
  queryClient.setQueryData(keys.detail('x1'), { id: 'x1', name: 'before' })
  return queryClient
}

function frame(entity: ChangeFrame['entity'], action: ChangeFrame['action'], data: unknown): ChangeFrame {
  return { type: 'change', entity, action, id: 'x1', data }
}

describe('applyChange', () => {
  describe.each(entities)('$entity', ({ entity, keys }) => {
    it('invalidates lists on create and leaves the detail cache alone', () => {
      const queryClient = seededClient(keys)

      applyChange(queryClient, frame(entity, 'create', { id: 'x2' }))

      expect(queryClient.getQueryState(keys.list(listParams))?.isInvalidated).toBe(true)
      expect(queryClient.getQueryData(keys.detail('x1'))).toEqual({ id: 'x1', name: 'before' })
    })

    it('sets the detail data and invalidates lists on update', () => {
      const queryClient = seededClient(keys)

      applyChange(queryClient, frame(entity, 'update', { id: 'x1', name: 'after' }))

      expect(queryClient.getQueryData(keys.detail('x1'))).toEqual({ id: 'x1', name: 'after' })
      expect(queryClient.getQueryState(keys.detail('x1'))?.isInvalidated).toBe(false)
      expect(queryClient.getQueryState(keys.list(listParams))?.isInvalidated).toBe(true)
    })

    it('removes the detail query and invalidates lists on delete', () => {
      const queryClient = seededClient(keys)

      applyChange(queryClient, frame(entity, 'delete', null))

      expect(queryClient.getQueryState(keys.detail('x1'))).toBeUndefined()
      expect(queryClient.getQueryState(keys.list(listParams))?.isInvalidated).toBe(true)
    })
  })

  it('ignores unknown entities', () => {
    const queryClient = seededClient(customersKeys)

    applyChange(queryClient, frame('machine' as ChangeFrame['entity'], 'create', null))

    expect(queryClient.getQueryState(customersKeys.list(listParams))?.isInvalidated).toBe(false)
  })
})

describe('handleServerFrame', () => {
  it('invalidates everything on resync', () => {
    const queryClient = seededClient(customersKeys)

    handleServerFrame(queryClient, { type: 'resync' })

    expect(queryClient.getQueryState(customersKeys.list(listParams))?.isInvalidated).toBe(true)
    expect(queryClient.getQueryState(customersKeys.detail('x1'))?.isInvalidated).toBe(true)
  })

  it('routes change frames through applyChange', () => {
    const queryClient = seededClient(customersKeys)

    handleServerFrame(queryClient, frame('customer', 'update', { id: 'x1', name: 'after' }))

    expect(queryClient.getQueryData(customersKeys.detail('x1'))).toEqual({ id: 'x1', name: 'after' })
  })
})
