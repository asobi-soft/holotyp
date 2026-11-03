// Ctrl-wheel zoom, reveal-by-page-point and the jump marker.
import type { PagePoint } from "@asobisoft/holotyp-preview-protocol"
import { nearestPage } from "./dom"
import { svgElement } from "./utils"

/**
 * How long the jump marker stays visible once the scroll has settled, in ms.
 */
const DWELL = 1200
const ZOOM_MIN = 0.25
const ZOOM_MAX = 4
const ZOOM_PER_PIXEL = 0.002
const MARKER_RADIUS = 12
/**
 * Frames without any scroll movement that count as a settled viewport.
 */
const SETTLE_FRAMES = 3

/**
 * The width the pages are fitted to: the viewport without its scrollbars, so a width
 * taken from here never adds a scrollbar of its own.
 */
function viewportWidth(): number {
  return document.documentElement.clientWidth
}

/**
 * The viewer's navigation layer: it owns the zoom factor and the jump marker and reads
 * page geometry.
 */
export class Navigation {
  /**
   * Zoom applied to the page column; 1 is the default fit-to-width factor.
   */
  private zoom = 1
  /**
   * The point the jump marker shows, kept across revisions so it can follow its page.
   */
  private highlighted: PagePoint | null = null
  private marker: SVGCircleElement | null = null
  private dwell: ReturnType<typeof setTimeout> | undefined
  private settleFrame: number | undefined
  /**
   * The wheel distance accumulated since the last frame, with the pointer that made it.
   */
  private pending: { delta: number; x: number; y: number } | null = null
  private zoomFrame: number | undefined
  private fitFrame: number | undefined

  constructor(
    /**
     * The stage that reserves the zoomed extent around the page column.
     */
    private readonly stage: () => HTMLElement | null,
    /**
     * The page column, i.e. the element that carries the zoom transform.
     */
    private readonly pageContainer: () => HTMLElement | null,
    /**
     * The page elements currently mounted, in page order.
     */
    private readonly pageElems: () => SVGSVGElement[],
  ) {}

  /**
   * Ctrl-wheel zooms the page column, keeping the point under the cursor in place.
   */
  readonly onWheel = (event: WheelEvent): void => {
    if (!event.ctrlKey) return
    // The listener must be non-passive, otherwise the browser zooms its own UI.
    event.preventDefault()
    // 0 = DOM_DELTA_PIXEL, 1 = DOM_DELTA_LINE, 2 = DOM_DELTA_PAGE
    const delta =
      event.deltaY *
      (event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? innerHeight : 1)
    if (!delta) return
    // Only accumulate here: a wheel burst delivers many events per frame and each zoom
    // step costs a paint, so one step per frame is the smoothest the screen can show.
    const pending = this.pending
    this.pending = {
      delta: (pending?.delta ?? 0) + delta,
      x: event.clientX,
      y: event.clientY,
    }
    this.zoomFrame ??= requestAnimationFrame(this.flushWheel)
  }

  /**
   * Apply the wheel distance accumulated since the last frame.
   */
  private readonly flushWheel = (): void => {
    this.zoomFrame = undefined
    const pending = this.pending
    this.pending = null
    if (!pending) return
    const next = Math.max(
      ZOOM_MIN,
      Math.min(ZOOM_MAX, this.zoom * Math.exp(-pending.delta * ZOOM_PER_PIXEL)),
    )
    if (next === this.zoom) return
    const page = nearestPage(this.pageElems(), pending.y)
    const matrix = page?.getScreenCTM()
    if (!page || !matrix) return
    const anchor = new DOMPoint(pending.x, pending.y).matrixTransform(matrix.inverse())
    this.zoom = next
    this.applyZoom()
    const moved = page.getScreenCTM()
    if (!moved) return
    const point = anchor.matrixTransform(moved)
    window.scrollBy({
      left: point.x - pending.x,
      top: point.y - pending.y,
      behavior: "instant",
    })
  }

  /**
   * Write the zoom factor to the stage and the page column.
   */
  applyZoom(): void {
    const stage = this.stage()
    const pageContainer = this.pageContainer()
    if (!stage || !pageContainer) return
    // Fit to width: the column is exactly the viewport wide, so every resize re-fits the
    // pages, and the zoom factor scales that width instead of replacing it.
    const width = viewportWidth()
    pageContainer.style.width = `${width}px`
    const { zoom } = this
    if (zoom === 1) {
      stage.style.removeProperty("width")
      stage.style.removeProperty("height")
      pageContainer.style.removeProperty("transform")
      return
    }
    pageContainer.style.transform = `scale(${zoom})`
    stage.style.width = `${width * Math.max(1, zoom)}px`
    stage.style.height = `${pageContainer.offsetHeight * zoom}px`
  }

  /**
   * Re-fit the page column to the viewport, at most once per frame: a window drag
   * delivers many resize events and each fit lays the whole document out again.
   */
  refit(): void {
    if (this.fitFrame !== undefined) return
    this.fitFrame = requestAnimationFrame(() => {
      this.fitFrame = undefined
      this.applyZoom()
    })
  }

  /**
   * Drop the pending wheel and fit frames and the jump marker.
   */
  dispose(): void {
    if (this.zoomFrame !== undefined) cancelAnimationFrame(this.zoomFrame)
    if (this.fitFrame !== undefined) cancelAnimationFrame(this.fitFrame)
    this.zoomFrame = undefined
    this.fitFrame = undefined
    this.pending = null
    this.disposeMarker()
  }

  /**
   * Reveal one page point — not just the page's box — and mark it.
   */
  reveal(target: PagePoint): void {
    const { page: index, x, y } = target
    const page = this.pageElems()[index]
    const matrix = page?.getScreenCTM()
    if (!page || !matrix) return
    const point = new DOMPoint(x, y).matrixTransform(matrix)
    const smooth = !matchMedia("(prefers-reduced-motion: reduce)").matches
    window.scrollTo({
      left: window.scrollX + point.x - innerWidth / 2,
      top: window.scrollY + point.y - innerHeight / 2,
      behavior: smooth ? "smooth" : "instant",
    })
    this.showMarker(page, target, smooth)
  }

  /**
   * Re-attach or drop the jump marker.
   */
  follow(reset: boolean, pageCount: number): void {
    if (reset || !this.highlighted || this.highlighted.page >= pageCount) {
      this.disposeMarker()
    } else if (this.marker) {
      this.pageElems()[this.highlighted.page]?.append(this.marker)
    }
  }

  /**
   * Drop the jump marker and the scroll settle watcher behind it.
   */
  disposeMarker(): void {
    clearTimeout(this.dwell)
    if (this.settleFrame !== undefined) cancelAnimationFrame(this.settleFrame)
    this.settleFrame = undefined
    this.highlighted = null
    this.marker?.remove()
    this.marker = null
  }

  /**
   * Show a pulse marker on one page point.
   */
  private showMarker(page: SVGSVGElement, target: PagePoint, smooth: boolean): void {
    this.disposeMarker()
    const circle = svgElement("circle")
    circle.classList.add("ht-jump-highlight")
    circle.setAttribute("cx", String(target.x))
    circle.setAttribute("cy", String(target.y))
    circle.setAttribute("r", String(MARKER_RADIUS))
    circle.setAttribute("pointer-events", "none")
    circle.setAttribute("aria-hidden", "true")
    page.append(circle)
    this.marker = circle
    this.highlighted = target

    const finish = (): void => {
      this.settleFrame = undefined
      circle.style.animationPlayState = "running"
      this.dwell = setTimeout(() => this.disposeMarker(), DWELL)
    }
    if (!smooth) {
      finish()
      return
    }
    circle.style.animationPlayState = "paused"
    let lastX = window.scrollX
    let lastY = window.scrollY
    let stable = 0
    const settle = (): void => {
      stable = window.scrollX === lastX && window.scrollY === lastY ? stable + 1 : 0
      lastX = window.scrollX
      lastY = window.scrollY
      if (stable >= SETTLE_FRAMES) finish()
      else this.settleFrame = requestAnimationFrame(settle)
    }
    this.settleFrame = requestAnimationFrame(settle)
  }
}
