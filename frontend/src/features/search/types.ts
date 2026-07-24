export interface SearchHit {
  id: string
  label: string
  highlight: string
}

export interface SearchResults {
  customers: SearchHit[]
  quotes: SearchHit[]
  orders: SearchHit[]
  invoices: SearchHit[]
}

export const SEARCH_ENTITIES = ['customers', 'quotes', 'orders', 'invoices'] as const
export type SearchEntity = (typeof SEARCH_ENTITIES)[number]
