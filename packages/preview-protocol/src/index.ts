// The Holotyp preview protocol in TypeScript: the generated wire types, the
// frame codec, the transports and the handshake every consumer shares.

// `index.ts` and `version.ts` are written by `holotyp-preview-protocol`'s
// `export_bindings`; do not edit them here.
export * from "./generated/index.js"
export { PROTOCOL_VERSION } from "./generated/version.js"
export { DEFAULT_SETTINGS } from "./settings.js"
export { decodeFrame, encodeFrame } from "./frame.js"
export {
  messagePortTransport,
  webSocketTransport,
  type FrameTransport,
  type WebSocketConstructor,
  type WebSocketLike,
} from "./transport.js"
export {
  Channel,
  type ChannelHandlers,
  type ChannelOptions,
  type RejectReason,
} from "./channel.js"
