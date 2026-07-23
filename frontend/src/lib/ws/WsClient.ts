import type { ServerFrame } from './types'

export interface SocketLike {
  onopen: ((event: Event) => void) | null
  onmessage: ((event: MessageEvent) => void) | null
  onclose: ((event: CloseEvent) => void) | null
  send(data: string): void
  close(): void
}

export interface WsClientOptions {
  url: string
  getToken: () => Promise<string | null>
  onFrame: (frame: ServerFrame) => void
  onReopen: () => void
  createSocket?: (url: string) => SocketLike
}

const INITIAL_BACKOFF_MS = 1_000
const MAX_BACKOFF_MS = 30_000

export class WsClient {
  private readonly options: WsClientOptions
  private socket: SocketLike | null = null
  private backoffMs = INITIAL_BACKOFF_MS
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null
  private stopped = false
  private hasConnected = false

  constructor(options: WsClientOptions) {
    this.options = options
  }

  start(): void {
    void this.connect()
  }

  stop(): void {
    this.stopped = true
    if (this.reconnectTimer !== null) {
      clearTimeout(this.reconnectTimer)
      this.reconnectTimer = null
    }
    const socket = this.socket
    this.socket = null
    socket?.close()
  }

  private async connect(): Promise<void> {
    if (this.stopped) {
      return
    }
    // Fetched fresh on every attempt: Clerk rotates short-lived tokens, so a
    // token cached at construction time would be rejected after the first drop.
    const token = await this.options.getToken().catch((error: unknown) => {
      console.warn('WsClient: failed to fetch auth token; will retry', error)
      return null
    })
    if (this.stopped) {
      return
    }
    if (token === null) {
      this.scheduleReconnect()
      return
    }
    const createSocket = this.options.createSocket ?? ((url: string) => new WebSocket(url))
    const socket = createSocket(`${this.options.url}/api/ws?token=${encodeURIComponent(token)}`)
    this.socket = socket
    socket.onopen = () => {
      this.backoffMs = INITIAL_BACKOFF_MS
      if (this.hasConnected) {
        this.options.onReopen()
      }
      this.hasConnected = true
    }
    socket.onmessage = (event) => {
      let frame: ServerFrame
      try {
        frame = JSON.parse(String(event.data)) as ServerFrame
      } catch (error) {
        console.warn('WsClient: dropping unparseable server frame', error)
        return
      }
      if (frame.type === 'ping') {
        socket.send(JSON.stringify({ type: 'pong' }))
        return
      }
      this.options.onFrame(frame)
    }
    socket.onclose = () => {
      if (this.socket !== socket) {
        return
      }
      this.socket = null
      this.scheduleReconnect()
    }
  }

  private scheduleReconnect(): void {
    if (this.stopped || this.reconnectTimer !== null) {
      return
    }
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null
      void this.connect()
    }, this.backoffMs)
    this.backoffMs = Math.min(this.backoffMs * 2, MAX_BACKOFF_MS)
  }
}
