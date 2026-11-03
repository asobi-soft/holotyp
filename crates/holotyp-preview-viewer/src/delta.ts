import type {
  CopyRun,
  NewPage,
  PageEntry,
  SlotOp,
} from "@asobisoft/holotyp-preview-protocol"
import { bodyNode } from "./utils"
import type { Page, Reuse, Slot, Slots } from "@/model"

export function ensureBodyOf(items: Map<bigint, string>, id: bigint): string {
  const body = items.get(id)
  if (body === undefined) throw new Error(`item ${id} is not in the body cache`)
  return body
}

function takeSlot(reuse: Reuse, slot: Slot, dx = 0, dy = 0): Slot {
  const free = !reuse.slot.has(slot)
  reuse.slot.add(slot)
  if (free && dx === 0 && dy === 0) {
    return slot
  }
  return {
    node: free ? slot.node : (slot.node.cloneNode(true) as SVGGElement),
    x: slot.x + dx,
    y: slot.y + dy,
  }
}

function mapFree(reuse: Reuse, slots: Slots): boolean {
  if (reuse.slots.has(slots)) {
    return false
  }
  for (const slot of slots.values()) {
    if (reuse.slot.has(slot)) {
      return false
    }
  }
  return true
}

function takeSlots(reuse: Reuse, slots: Slots): Slots {
  const free = mapFree(reuse, slots)
  reuse.slots.add(slots)
  if (free) {
    for (const slot of slots.values()) reuse.slot.add(slot)
    return slots
  }
  const copy: Slots = new Map()
  for (const [index, slot] of slots) {
    copy.set(index, takeSlot(reuse, slot))
  }
  return copy
}

function takePage(reuse: Reuse, page: Page): Page {
  const free = !reuse.page.has(page) && mapFree(reuse, page.slots)
  reuse.page.add(page)
  const slots = takeSlots(reuse, page.slots)
  if (free) {
    return page
  }
  return { size: page.size, background: page.background, slots }
}

function applyOps(
  reuse: Reuse,
  page: Page | undefined,
  ops: SlotOp[],
  items: Map<bigint, string>,
): Slots {
  const slots: Slots = new Map()
  for (const op of ops) {
    if ("reuse" in op) {
      const { at, from, count, dx, dy } = op.reuse
      for (let i = 0; i < count; i++) {
        const prior = page?.slots.get(from + i)
        if (prior) slots.set(at + i, takeSlot(reuse, prior, dx, dy))
      }
    } else if ("place" in op) {
      const { at, id, x, y } = op.place
      slots.set(at, { node: bodyNode(ensureBodyOf(items, id)), x, y })
    }
  }
  return slots
}

function newPage(
  previous: Page[],
  entry: NewPage,
  items: Map<bigint, string>,
  reuse: Reuse,
): Page {
  const { from, size, background, layout } = entry
  const prior = from === null ? undefined : previous[from]
  const pageSize = size ?? prior?.size
  if (pageSize === undefined) {
    // Only a client whose page list is out of step can get here: the host always sends a
    // size when it names no previous page.
    throw new Error(`page has no size and no previous page (from: ${from})`)
  }
  const newSlots: Slots =
    layout === "same"
      ? prior === undefined
        ? new Map()
        : takeSlots(reuse, prior.slots)
      : applyOps(reuse, prior, layout.ops, items)
  return { size: pageSize, background, slots: newSlots }
}

/**
 * One `PageEntry::Copy`: pages `from ... from + count` of the previous list.
 */
function copiedPages(previous: Page[], run: CopyRun, reuse: Reuse): Page[] {
  const pages: Page[] = []
  for (let i = 0; i < run.count; i++) {
    const page = previous[run.from + i]
    if (page) {
      pages.push(takePage(reuse, page))
    }
  }
  return pages
}

/**
 * The page list a delta produces.
 */
export function nextPages(
  previous: Page[],
  entries: PageEntry[],
  items: Map<bigint, string>,
): Page[] {
  const reuse: Reuse = { page: new Set(), slots: new Set(), slot: new Set() }
  const pages: Page[] = []
  for (const entry of entries) {
    if ("copy" in entry) {
      pages.push(...copiedPages(previous, entry.copy, reuse))
    } else {
      pages.push(newPage(previous, entry.new, items, reuse))
    }
  }
  return pages
}
