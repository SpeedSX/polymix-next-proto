import type { ReactNode } from 'react'

import { ClerkAuthProvider } from './ClerkAuthProvider'
import { isClerkMode } from './context'
import { DevAuthProvider } from './DevAuthProvider'

export function AuthProvider({ children }: { children: ReactNode }) {
  return isClerkMode() ? (
    <ClerkAuthProvider>{children}</ClerkAuthProvider>
  ) : (
    <DevAuthProvider>{children}</DevAuthProvider>
  )
}
