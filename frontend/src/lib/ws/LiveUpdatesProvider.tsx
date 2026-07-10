import { useEffect, useRef } from 'react'
import type { ReactNode } from 'react'
import { useQueryClient } from '@tanstack/react-query'

import { useAuth } from '../auth'
import { handleServerFrame } from './applyChange'
import { WsClient } from './WsClient'

const WS_URL: string = import.meta.env.VITE_WS_URL ?? ''

function LiveUpdatesSocket() {
  const { getToken } = useAuth()
  const queryClient = useQueryClient()
  const getTokenRef = useRef(getToken)

  useEffect(() => {
    getTokenRef.current = getToken
  })

  useEffect(() => {
    const client = new WsClient({
      url: WS_URL,
      getToken: () => getTokenRef.current(),
      onFrame: (frame) => handleServerFrame(queryClient, frame),
      // Events sent while disconnected are gone — refetch everything rather
      // than leave stale UI.
      onReopen: () => void queryClient.invalidateQueries(),
    })
    client.start()
    return () => client.stop()
  }, [queryClient])

  return null
}

export function LiveUpdatesProvider({ children }: { children: ReactNode }) {
  const { orgId } = useAuth()

  return (
    <>
      {/* Keyed by org: an org switch changes the tenant, and a surviving
          socket would keep streaming the previous tenant's events. */}
      <LiveUpdatesSocket key={orgId} />
      {children}
    </>
  )
}
