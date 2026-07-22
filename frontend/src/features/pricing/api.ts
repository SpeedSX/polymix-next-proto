import type { useApi } from '../../lib/api'
import type { CatalogDoc, PricingEntitySegment } from './types'

type Api = ReturnType<typeof useApi>

interface ListResponse {
  items: CatalogDoc[]
}

interface VersionResponse {
  version: number
}

export const pricingKeys = {
  all: ['pricing'] as const,
  version: () => ['pricing', 'version'] as const,
  list: (entity: PricingEntitySegment) => ['pricing', entity] as const,
  detail: (entity: PricingEntitySegment, id: string) => ['pricing', entity, id] as const,
}

// Record ids carry a colon (`format:a5`); encode so it survives the path.
function encodeId(id: string): string {
  return encodeURIComponent(id)
}

export function fetchVersion(api: Api) {
  return api<VersionResponse>('/api/pricing/version')
}

export function fetchList(api: Api, entity: PricingEntitySegment) {
  return api<ListResponse>(`/api/pricing/${entity}`)
}

export function fetchOne(api: Api, entity: PricingEntitySegment, id: string) {
  return api<CatalogDoc>(`/api/pricing/${entity}/${encodeId(id)}`)
}

export function createEntity(api: Api, entity: PricingEntitySegment, doc: CatalogDoc) {
  return api<CatalogDoc>(`/api/pricing/${entity}`, { method: 'POST', body: doc })
}

export function updateEntity(api: Api, entity: PricingEntitySegment, id: string, doc: CatalogDoc) {
  return api<CatalogDoc>(`/api/pricing/${entity}/${encodeId(id)}`, { method: 'PUT', body: doc })
}

export function deleteEntity(api: Api, entity: PricingEntitySegment, id: string) {
  return api<void>(`/api/pricing/${entity}/${encodeId(id)}`, { method: 'DELETE' })
}
