import { ApiError } from './ApiError'
import type { ApiErrorBody } from './ApiError'

const API_URL: string = import.meta.env.VITE_API_URL ?? ''

export interface FetchJsonOptions {
  method?: 'GET' | 'POST' | 'PUT' | 'DELETE'
  body?: unknown
  params?: Record<string, string | number | undefined>
  getToken: () => Promise<string | null>
}

function buildUrl(path: string, params: FetchJsonOptions['params']): string {
  const url = new URL(`${API_URL}${path}`)
  for (const [key, value] of Object.entries(params ?? {})) {
    if (value !== undefined) {
      url.searchParams.set(key, String(value))
    }
  }
  return url.toString()
}

export async function fetchJson<T>(path: string, options: FetchJsonOptions): Promise<T> {
  const token = await options.getToken()
  const headers: Record<string, string> = {}
  if (token) {
    headers.authorization = `Bearer ${token}`
  }
  if (options.body !== undefined) {
    headers['content-type'] = 'application/json'
  }

  const response = await fetch(buildUrl(path, options.params), {
    method: options.method ?? 'GET',
    headers,
    body: options.body !== undefined ? JSON.stringify(options.body) : undefined,
  })

  if (!response.ok) {
    const payload = (await response.json().catch(() => null)) as { error?: ApiErrorBody } | null
    if (payload?.error) {
      throw new ApiError(response.status, payload.error)
    }
    throw new ApiError(response.status, { code: 'internal', message: response.statusText })
  }

  if (response.status === 204) {
    return undefined as T
  }

  return (await response.json()) as T
}
