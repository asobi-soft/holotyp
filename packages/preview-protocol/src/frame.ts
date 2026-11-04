// The wire frame: one deflated CBOR map, as `holotyp-preview-protocol` writes it.

import { decode, encode } from "cbor-x"
import { deflateSync } from "fflate"
import type { Frame } from "./generated/index.js"

const COMPRESSION = "deflate-raw"

/**
 * The lowest deflate level, for frames a client sends.
 */
const LEVEL = 0

export async function decodeFrame(bytes: Uint8Array): Promise<Frame> {
  const stream = new Blob([Uint8Array.from(bytes)])
    .stream()
    .pipeThrough(new DecompressionStream(COMPRESSION))
  return decode(new Uint8Array(await new Response(stream).arrayBuffer())) as Frame
}

export async function encodeFrame(frame: Frame): Promise<Uint8Array<ArrayBuffer>> {
  return deflateSync(encode(frame), { level: LEVEL })
}
