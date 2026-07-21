import { useCallback } from 'react'

import { useAuth } from '../auth'
import { ApiError } from './ApiError'
import { fetchJson } from './fetchJson'
import type { FetchJsonOptions } from './fetchJson'

export function useApi() {
  const { getToken, signOut } = useAuth()

  return useCallback(
    async <T>(path: string, options?: Omit<FetchJsonOptions, 'getToken'>): Promise<T> => {
      try {
        return await fetchJson<T>(path, { ...options, getToken })
      } catch (error) {
        // A stale cached token (e.g. one signed by a previous dev-server
        // instance, or a server-revoked Clerk session) is unrecoverable —
        // drop the session so the app falls back to sign-in instead of
        // retrying the dead token on every request.
        if (error instanceof ApiError && error.status === 401) {
          signOut()
        }
        throw error
      }
    },
    [getToken, signOut],
  )
}
