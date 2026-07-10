import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { WsClient } from './WsClient'
import type { SocketLike } from './WsClient'

class MockSocket implements SocketLike {
  onopen: ((event: Event) => void) | null = null
  onmessage: ((event: MessageEvent) => void) | null = null
  onclose: ((event: CloseEvent) => void) | null = null
  sent: string[] = []
  closed = false
  url: string

  constructor(url: string) {
    this.url = url
  }

  send(data: string) {
    this.sent.push(data)
  }

  close() {
    this.closed = true
    this.onclose?.(new Event('close') as CloseEvent)
  }

  open() {
    this.onopen?.(new Event('open'))
  }

  message(frame: unknown) {
    this.onmessage?.({ data: JSON.stringify(frame) } as MessageEvent)
  }

  drop() {
    this.onclose?.(new Event('close') as CloseEvent)
  }
}

function createHarness() {
  const sockets: MockSocket[] = []
  const getToken = vi.fn(async (): Promise<string | null> => 'token')
  const onFrame = vi.fn()
  const onReopen = vi.fn()
  const client = new WsClient({
    url: 'ws://test',
    getToken,
    onFrame,
    onReopen,
    createSocket: (url) => {
      const socket = new MockSocket(url)
      sockets.push(socket)
      return socket
    },
  })
  return { client, sockets, getToken, onFrame, onReopen }
}

describe('WsClient', () => {
  beforeEach(() => {
    vi.useFakeTimers()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('connects with the token in the query string', async () => {
    const { client, sockets } = createHarness()

    client.start()
    await vi.advanceTimersByTimeAsync(0)

    expect(sockets).toHaveLength(1)
    expect(sockets[0].url).toBe('ws://test/api/ws?token=token')
  })

  it('replies pong to pings without surfacing them as frames', async () => {
    const { client, sockets, onFrame } = createHarness()
    client.start()
    await vi.advanceTimersByTimeAsync(0)
    sockets[0].open()

    sockets[0].message({ type: 'ping' })

    expect(sockets[0].sent).toEqual([JSON.stringify({ type: 'pong' })])
    expect(onFrame).not.toHaveBeenCalled()
  })

  it('forwards change and resync frames', async () => {
    const { client, sockets, onFrame } = createHarness()
    client.start()
    await vi.advanceTimersByTimeAsync(0)
    sockets[0].open()

    const change = { type: 'change', entity: 'customer', action: 'create', id: 'x1', data: { id: 'x1' } }
    sockets[0].message(change)
    sockets[0].message({ type: 'resync' })

    expect(onFrame).toHaveBeenNthCalledWith(1, change)
    expect(onFrame).toHaveBeenNthCalledWith(2, { type: 'resync' })
  })

  it('reconnects with exponential backoff and a fresh token per attempt', async () => {
    const { client, sockets, getToken } = createHarness()
    client.start()
    await vi.advanceTimersByTimeAsync(0)
    sockets[0].open()

    sockets[0].drop()
    await vi.advanceTimersByTimeAsync(999)
    expect(sockets).toHaveLength(1)
    await vi.advanceTimersByTimeAsync(1)
    expect(sockets).toHaveLength(2)

    sockets[1].drop()
    await vi.advanceTimersByTimeAsync(1_999)
    expect(sockets).toHaveLength(2)
    await vi.advanceTimersByTimeAsync(1)
    expect(sockets).toHaveLength(3)

    expect(getToken).toHaveBeenCalledTimes(3)
  })

  it('caps the backoff at 30s', async () => {
    const { client, sockets } = createHarness()
    client.start()
    await vi.advanceTimersByTimeAsync(0)

    for (let attempt = 0; attempt < 8; attempt += 1) {
      sockets[sockets.length - 1].drop()
      await vi.advanceTimersByTimeAsync(30_000)
    }
    const before = sockets.length

    sockets[sockets.length - 1].drop()
    await vi.advanceTimersByTimeAsync(29_999)
    expect(sockets).toHaveLength(before)
    await vi.advanceTimersByTimeAsync(1)
    expect(sockets).toHaveLength(before + 1)
  })

  it('calls onReopen and resets the backoff on reconnect, but not on the first open', async () => {
    const { client, sockets, onReopen } = createHarness()
    client.start()
    await vi.advanceTimersByTimeAsync(0)

    sockets[0].open()
    expect(onReopen).not.toHaveBeenCalled()

    sockets[0].drop()
    await vi.advanceTimersByTimeAsync(1_000)
    sockets[1].drop()
    await vi.advanceTimersByTimeAsync(2_000)
    sockets[2].open()
    expect(onReopen).toHaveBeenCalledTimes(1)

    sockets[2].drop()
    await vi.advanceTimersByTimeAsync(999)
    expect(sockets).toHaveLength(3)
    await vi.advanceTimersByTimeAsync(1)
    expect(sockets).toHaveLength(4)
  })

  it('stops reconnecting after stop()', async () => {
    const { client, sockets } = createHarness()
    client.start()
    await vi.advanceTimersByTimeAsync(0)
    sockets[0].open()

    client.stop()

    expect(sockets[0].closed).toBe(true)
    await vi.advanceTimersByTimeAsync(120_000)
    expect(sockets).toHaveLength(1)
  })

  it('retries when no token is available instead of connecting', async () => {
    const { client, sockets, getToken } = createHarness()
    getToken.mockResolvedValueOnce(null)

    client.start()
    await vi.advanceTimersByTimeAsync(0)
    expect(sockets).toHaveLength(0)

    await vi.advanceTimersByTimeAsync(1_000)
    expect(sockets).toHaveLength(1)
    expect(getToken).toHaveBeenCalledTimes(2)
  })
})
