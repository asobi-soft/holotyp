// DOM utils
import type { PagePoint } from "@asobisoft/holotyp-preview-protocol"
import type { Page } from "@/model"
import { ensureBodyOf } from "./delta"
import { bodyNode, svgElement } from "./utils"

/**
 * Create a new page `<svg>`
 */
export function pageElement(
  index: number,
  page: Page,
  items: Map<bigint, string>,
): SVGSVGElement {
  const element = svgElement("svg")
  element.classList.add("ht-page")
  element.dataset.page = String(index)
  element.setAttribute(
    "viewBox",
    `0 0 ${page.size.width.toFixed(2)} ${page.size.height.toFixed(2)}`,
  )

  patchPage(element, index, page, undefined, items)
  return element
}

function patchPage(
  element: SVGSVGElement,
  index: number,
  page: Page,
  previous: Page | undefined,
  items: Map<bigint, string>,
): void {
  const pageIndex = String(index)

  // update page no
  if (element.dataset.page !== pageIndex) {
    element.dataset.page = pageIndex
  }

  // update size
  const viewBox = `0 0 ${page.size.width.toFixed(2)} ${page.size.height.toFixed(2)}`
  if (element.getAttribute("viewBox") !== viewBox)
    element.setAttribute("viewBox", viewBox)

  // for a newly created page, fill content
  let content = element.querySelector<SVGGElement>(":scope > .ht-content")
  if (!content) {
    content = svgElement("g")
    content.classList.add("ht-content")
    element.append(content)
  }

  const children: Element[] = []
  // bkg
  if (page.background !== null) {
    const old = content.querySelector<SVGGElement>(":scope > .ht-bg")
    const background =
      old && previous?.background === page.background
        ? old
        : bodyNode(ensureBodyOf(items, page.background))
    background.classList.add("ht-bg")
    children.push(background)
  }

  // slots
  for (const [at, slot] of [...page.slots].sort(([a], [b]) => a - b)) {
    // update index
    const index = String(at)
    if (slot.node.dataset.slot !== index) slot.node.dataset.slot = index

    // update pos
    const transform = `translate(${slot.x.toFixed(3)},${slot.y.toFixed(3)})`
    if (slot.node.getAttribute("transform") !== transform)
      slot.node.setAttribute("transform", transform)

    children.push(slot.node)
  }

  // push `children` into `content`
  const live = new Set(children)
  for (const child of [...content.children]) {
    if (!live.has(child)) child.remove()
  }
  let cursor = content.firstElementChild
  for (const child of children) {
    if (child === cursor) cursor = cursor.nextElementSibling
    else content.insertBefore(child, cursor)
  }
}

/**
 * Patch changed pages in place and leave copied pages attached.
 */
export function mountPages(
  pageContainer: HTMLElement,
  pages: Page[],
  previousPages: Page[],
  previousElements: SVGSVGElement[],
  items: Map<bigint, string>,
): SVGSVGElement[] {
  const prior = new Map<Page, SVGSVGElement>()
  const priorRev = new Map<SVGSVGElement, Page>()
  for (let i = 0; i < previousPages.length; i++) {
    const page = previousPages[i]
    const element = previousElements[i]
    if (page && element) {
      prior.set(page, element)
      priorRev.set(element, page)
    }
  }
  // Reserve copied pages before assigning containers to changed pages.
  const used = new Set(
    pages
      .map((page) => prior.get(page))
      .filter((element): element is SVGSVGElement => element !== undefined),
  )
  const elements = pages.map((page, index) => {
    const copied = prior.get(page)
    if (copied) {
      if (copied.dataset.page !== String(index)) copied.dataset.page = String(index)
      // used is already calculated
      return copied
    }
    const candidate = previousElements[index]
    if (candidate && !used.has(candidate)) {
      used.add(candidate)
      patchPage(candidate, index, page, priorRev.get(candidate), items)
      return candidate
    }
    return pageElement(index, page, items)
  })

  // push `elements` into `pageContainer`
  const live = new Set(elements)
  for (const element of previousElements) if (!live.has(element)) element.remove()
  let cursor: Element | null = pageContainer.querySelector(":scope > .ht-page")
  for (const element of elements) {
    if (element === cursor) cursor = cursor.nextElementSibling
    else pageContainer.insertBefore(element, cursor)
  }
  return elements
}

/**
 * The point a click names, or null for a click that misses every page.
 */
export function pagePointAt(event: MouseEvent): PagePoint | null {
  if (!(event.target instanceof Element)) return null
  const page = event.target.closest<SVGSVGElement>("svg.ht-page")
  const matrix = page?.getScreenCTM()
  if (!page || !matrix) return null
  const { x, y } = new DOMPoint(event.clientX, event.clientY).matrixTransform(
    matrix.inverse(),
  )
  const box = page.viewBox.baseVal
  const index = Number(page.dataset.page)
  const inside =
    x >= box.x && y >= box.y && x <= box.x + box.width && y <= box.y + box.height
  if (!inside) return null
  return { page: index, x, y }
}

/**
 * The page whose vertical band is closest to `clientY`, or null for an empty document.
 */
export function nearestPage(
  pages: SVGSVGElement[],
  clientY: number,
): SVGSVGElement | null {
  let nearest: SVGSVGElement | null = null
  for (const page of pages) {
    if (!nearest || gapTo(page, clientY) < gapTo(nearest, clientY)) nearest = page
  }
  return nearest
}

/**
 * Vertical distance from `clientY` to a page, 0 while the pointer is over it.
 */
function gapTo(page: SVGSVGElement, clientY: number): number {
  const box = page.getBoundingClientRect()
  return Math.max(box.top - clientY, 0, clientY - box.bottom)
}
