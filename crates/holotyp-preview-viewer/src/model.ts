import type { PageSize } from "@asobisoft/holotyp-preview-protocol"

/**
 * One placed item: the body node and the page position it is translated to.
 */
export interface Slot {
  node: SVGGElement
  x: number
  y: number
}

/**
 * One page's slots, by slot index.
 */
export type Slots = Map<number, Slot>

/**
 * A page of one revision: its size, its background body and its slots.
 */
export interface Page {
  size: PageSize
  /**
   * Id of the body that paints the page background; null = none.
   */
  background: bigint | null
  slots: Slots
}

/**
 * What one revision has reused so far.
 */
export interface Reuse {
  page: Set<Page>
  slots: Set<Slots>
  slot: Set<Slot>
}

/**
 * What the status line reports: nothing arrived yet, or the revision committed last.
 */
export class Status {
  /**
   * The revision this status reports; null = nothing arrived yet.
   */
  readonly revision: bigint | null

  private constructor(revision: bigint | null) {
    this.revision = revision
  }

  /**
   * Nothing has been committed yet.
   */
  static readonly waiting = new Status(null)

  /**
   * The revision the viewer committed last.
   */
  static rev(revision: bigint): Status {
    return new Status(revision)
  }

  /**
   * The status line's text
   */
  toString(): string {
    return this.revision === null ? "waiting for data" : `rev ${this.revision}`
  }
}
