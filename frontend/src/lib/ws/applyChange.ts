import type { QueryClient } from '@tanstack/react-query'

import { customersKeys } from '../../features/customers/api'
import { invoicesKeys } from '../../features/invoices/api'
import { ordersKeys } from '../../features/orders/api'
import type { ChangeFrame, ServerFrame } from './types'

interface EntityKeys {
  all: readonly string[]
  detail: (id: string) => readonly unknown[]
}

// Unknown entities are ignored on purpose — a backend deployed ahead of the
// frontend must not break connected clients.
const KEYS_BY_ENTITY: Record<string, EntityKeys> = {
  customer: customersKeys,
  order: ordersKeys,
  invoice: invoicesKeys,
}

export function applyChange(queryClient: QueryClient, frame: ChangeFrame): void {
  const keys = KEYS_BY_ENTITY[frame.entity]
  if (!keys) {
    return
  }
  switch (frame.action) {
    case 'create':
      void queryClient.invalidateQueries({ queryKey: keys.all })
      break
    case 'update':
      if (frame.data == null) {
        void queryClient.invalidateQueries({ queryKey: keys.all })
        break
      }
      queryClient.setQueryData(keys.detail(frame.id), frame.data)
      // The detail cache was just set to the server truth — invalidating it
      // too would refetch what the frame already delivered.
      void queryClient.invalidateQueries({
        queryKey: keys.all,
        predicate: (query) => query.queryKey[1] !== frame.id,
      })
      break
    case 'delete':
      queryClient.removeQueries({ queryKey: keys.detail(frame.id) })
      void queryClient.invalidateQueries({ queryKey: keys.all })
      break
  }
}

export function handleServerFrame(queryClient: QueryClient, frame: ServerFrame): void {
  if (frame.type === 'change') {
    applyChange(queryClient, frame)
  } else if (frame.type === 'resync') {
    void queryClient.invalidateQueries()
  }
}
