// The byte channel to a preview host.

/**
 * One connection that carries whole frames, one message per frame.
 */
export interface FrameTransport {
  listen(onFrame: (frame: Uint8Array) => void): void
  send(frame: Uint8Array<ArrayBuffer>): void
  close(): void
}

/**
 * What a transport needs from a WebSocket. The browser's `WebSocket` and the `ws` package
 * both satisfy it; the fields stay loose on purpose, because the two disagree on their
 * exact types.
 */
export interface WebSocketLike {
  binaryType: string
  readonly readyState: number
  send(data: unknown): void
  close(code?: number, reason?: string): void
  /**
   * Only `event.data` is read; see the note above.
   */
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  onmessage: ((event: any) => void) | null
}

/**
 * A `WebSocket` constructor. Pass `ws` where the host has no global one, as in a VS Code
 * extension host before Node 22.
 */
export type WebSocketConstructor = new (url: string) => WebSocketLike

/**
 * `WebSocket.OPEN`, from the spec, so any implementation can be passed in.
 */
const OPEN = 1

/**
 * The browser path: `typst preview` serves frames over `/ws`.
 *
 * `create` defaults to the global `WebSocket`; a host without one (a VS Code extension
 * host before Node 22) passes `ws`'s `WebSocket`.
 */
export function webSocketTransport(
  url: string,
  create: WebSocketConstructor | undefined = undefined,
): FrameTransport {
  const socket = new (create ?? globalWebSocket())(url)
  socket.binaryType = "arraybuffer"
  return {
    listen(onFrame) {
      socket.onmessage = (event) => onFrame(new Uint8Array(event.data))
    },
    send(frame) {
      if (socket.readyState === OPEN) socket.send(frame)
    },
    close() {
      socket.close()
    },
  }
}

/**
 * The wasm path: the engine runs in a worker and frames ride `postMessage`.
 */
export function messagePortTransport(port: MessagePort): FrameTransport {
  return {
    listen(onFrame) {
      port.onmessage = (event: MessageEvent<Uint8Array | ArrayBuffer>) =>
        onFrame(
          event.data instanceof Uint8Array ? event.data : new Uint8Array(event.data),
        )
    },
    send(frame) {
      port.postMessage(frame)
    },
    close() {
      port.close()
    },
  }
}

function globalWebSocket(): WebSocketConstructor {
  const impl = (globalThis as { WebSocket?: unknown }).WebSocket
  if (typeof impl !== "function") {
    throw new Error("this host has no global WebSocket; pass one, for instance `ws`")
  }
  return impl as WebSocketConstructor
}
