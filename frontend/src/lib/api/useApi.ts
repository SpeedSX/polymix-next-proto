import { useCallback } from 'react'

import { useAuth } from '../auth'
import { fetchJson } from './fetchJson'
import type { FetchJsonOptions } from './fetchJson'

export function useApi() {
  const { getToken } = useAuth()

  return useCallback(
    <T>(path: string, options?: Omit<FetchJsonOptions, 'getToken'>) =>
      fetchJson<T>(path, { ...options, getToken }),
    [getToken],
  )
}
