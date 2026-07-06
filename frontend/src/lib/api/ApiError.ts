export interface ApiErrorBody {
  code: string
  message: string
  details?: Record<string, string>
}

export class ApiError extends Error {
  status: number
  code: string
  details?: Record<string, string>

  constructor(status: number, body: ApiErrorBody) {
    super(body.message)
    this.name = 'ApiError'
    this.status = status
    this.code = body.code
    this.details = body.details
  }
}
