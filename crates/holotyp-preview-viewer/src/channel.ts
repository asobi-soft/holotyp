import {
  Channel,
  type Delta,
  type FrameTransport,
  type Message,
  type PagePoint,
  type Role,
} from "@asobisoft/holotyp-preview-protocol"

/**
 * What a session hands its owner; the handshake and the encoding stay inside.
 */
export interface PreviewHandlers {
  /**
   * One decoded revision.
   */
  onDelta: (delta: Delta) => void
  /**
   * A broadcast jump target.
   */
  onScrollTo: (target: PagePoint) => void
}

/**
 * One Holotyp Preview session, as the viewer sees it.
 */
export class PreviewChannel {
  private readonly channel: Channel

  constructor(
    transport: FrameTransport,
    handlers: PreviewHandlers,
    role: Role = "viewer",
  ) {
    this.channel = new Channel(transport, {
      role,
      onMessage: (message: Message) => {
        if ("delta" in message) handlers.onDelta(message.delta)
        else if ("scrollTo" in message) handlers.onScrollTo(message.scrollTo)
      },
    })
  }

  /**
   * Set once the host's `Hello` passed.
   */
  get ready(): boolean {
    return this.channel.ready
  }

  send(message: Message): Promise<boolean> {
    return this.channel.send(message)
  }

  close(): void {
    this.channel.close()
  }
}
