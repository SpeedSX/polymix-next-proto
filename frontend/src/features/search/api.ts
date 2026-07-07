import type { useApi } from '../../lib/api'
import type { SearchResults } from './types'

type Api = ReturnType<typeof useApi>

export const searchKeys = {
  query: (q: string) => ['search', q] as const,
}

export function fetchSearch(api: Api, q: string) {
  return api<SearchResults>('/api/search', { params: { q } })
}
