// The handshake and the message stream every preview consumer shares.

import type { Frame, Message, Role } from "./generated/index.js"
import { PROTOCOL_VERSION } from "./generated/version.js"
import { decodeFrame, encodeFrame } from "./frame.js"
import type { FrameTransport } from "./transport.js"

/**
 * Why a peer's `Hello` was refused.
 */
export type RejectReason = "protocol" | "role"

/**
 * What a channel reports to its owner; the handshake itself stays inside.
 */
export interface ChannelHandlers {
  /**
   * The peer's `Hello` passed and ours is on its way. Everything sent from here is queued
   * behind the `Hello`.
   */
  onReady?(): void
  /**
   * One message from the peer, in receive order. Never a `Hello`, and never anything that
   * arrived before the handshake settled.
   */
  onMessage?(message: Message): void
  /**
   * The peer speaks another protocol version or announces the wrong role; the channel
   * closed itself.
   */
  onReject?(reason: RejectReason): void
  /**
   * A frame failed to decode or encode. It was dropped; the channel stays open. Defaults
   * to logging the error.
   */
  onError?(error: unknown): void
}

export interface ChannelOptions extends ChannelHandlers {
  /**
   * The role this end announces in its `Hello`.
   */
  role: Role
  /**
   * The role the peer must announce; defaults to `"server"`.
   */
  peer?: Role
}

/**
 * One connection's message stream.
 */
export class Channel {
  /**
   * Number of the next frame this end sends.
   */
  private sequence = 0
  /**
   * Receive chain.
   */
  private inbox: Promise<void> = Promise.resolve()
  /**
   * Send chain.
   */
  private outbox: Promise<void> = Promise.resolve()
  private closed = false
  /**
   * Set once the peer's `Hello` passed.
   */
  ready = false

  constructor(
    private readonly transport: FrameTransport,
    private readonly options: ChannelOptions,
  ) {
    transport.listen((frame) => {
      this.inbox = this.inbox
        .then(() => this.receive(frame))
        .catch((error: unknown) => {
          this.report(error)
        })
    })
  }

  /**
   * Encode and send one message with the next sequence number.
   *
   * Resolves `false` when the handshake has not passed or the channel is closed: nothing
   * is queued for the peer before the `Hello` is settled.
   */
  send(message: Message): Promise<boolean> {
    if (this.closed || !this.ready) return Promise.resolve(false)
    const frame: Frame = { seq: ++this.sequence, message }
    const sent = this.outbox
      .then(() => encodeFrame(frame))
      .then((bytes) => {
        if (this.closed) return false
        this.transport.send(bytes)
        return true
      })
    // Keep the chain alive after a failure, so one bad frame does not stop the
    // ones behind it.
    this.outbox = sent.then(
      () => undefined,
      () => undefined,
    )
    return sent.catch((error: unknown) => {
      this.report(error)
      return false
    })
  }

  close(): void {
    if (this.closed) return
    this.closed = true
    this.ready = false
    this.transport.close()
  }

  private async receive(frame: Uint8Array): Promise<void> {
    if (this.closed) return
    const { message } = await decodeFrame(frame)
    if ("hello" in message) {
      this.acceptHello(message.hello)
      return
    }
    if (!this.ready) return
    this.options.onMessage?.(message)
  }

  private acceptHello(hello: { protocol: number; role: Role }): void {
    // A repeated `Hello` is a no-op, not a second handshake.
    if (this.closed || this.ready) return
    if (hello.protocol !== PROTOCOL_VERSION) return this.reject("protocol")
    if (hello.role !== (this.options.peer ?? "server")) return this.reject("role")
    this.ready = true
    void this.send({ hello: { protocol: PROTOCOL_VERSION, role: this.options.role } })
    this.options.onReady?.()
  }

  private reject(reason: RejectReason): void {
    this.close()
    this.options.onReject?.(reason)
  }

  private report(error: unknown): void {
    if (this.options.onError) this.options.onError(error)
    else console.error("preview frame dropped", error)
  }
}
