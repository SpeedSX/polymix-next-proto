import { createContext, useContext } from 'react'

export type AuthMode = 'clerk' | 'dev'

export interface AuthContextValue {
  mode: AuthMode
  orgId: string
  getToken: () => Promise<string | null>
  signOut: () => void
}

export const AuthContext = createContext<AuthContextValue | null>(null)

export function isClerkMode(): boolean {
  return import.meta.env.VITE_AUTH_MODE === 'clerk'
}

export function useAuth(): AuthContextValue {
  const ctx = useContext(AuthContext)
  if (!ctx) {
    throw new Error('useAuth must be used within an AuthProvider')
  }
  return ctx
}
