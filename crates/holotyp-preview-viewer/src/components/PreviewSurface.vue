<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, useTemplateRef } from "vue"
import { PreviewChannel } from "@/channel"
import { nextPages } from "@/delta"
import { mountPages, pagePointAt } from "@/dom"
import { type Page, Status } from "@/model"
import { Navigation } from "@/navigation"
import type {
  Delta,
  Def,
  FrameTransport,
  PagePoint,
  PreviewSettings,
} from "@asobisoft/holotyp-preview-protocol"

const props = defineProps<{
  transport: FrameTransport
  settings: PreviewSettings
}>()

const status = ref(Status.waiting)

let channel: PreviewChannel | null = null

const stage = useTemplateRef<HTMLDivElement>("stage")
const pageContainer = useTemplateRef<HTMLDivElement>("pageContainer")
let pageElems: SVGSVGElement[] = []
let pages: Page[] = []

const defs = useTemplateRef<SVGDefsElement>("defs")
const items = new Map<bigint, string>()

const navigation = new Navigation(
  () => stage.value,
  () => pageContainer.value,
  () => pageElems,
)

/**
 * Add the def this client does not have yet.
 */
function addDef(def: Def): void {
  const host = defs.value
  if (!host) return
  if (host.querySelector(`[id="${CSS.escape(def.id)}"]`)) return
  host.insertAdjacentHTML("beforeend", def.svg)
}

/**
 * Commit one `Delta`, i.e. one revision.
 */
function applyDelta(delta: Delta): void {
  for (const def of delta.defs) addDef(def)
  for (const item of delta.items) items.set(item.id, item.svg)

  const previous = delta.reset ? [] : pages
  const nxtPages = nextPages(previous, delta.pages, items)
  if (pageContainer.value)
    pageElems = mountPages(pageContainer.value, nxtPages, previous, pageElems, items)
  pages = nxtPages
  // The stage reserves the zoomed extent, which the new pages change.
  navigation.applyZoom()
  navigation.follow(delta.reset, nxtPages.length)
  status.value = Status.rev(delta.revision)
}

/**
 * A click on a link lands on the page: an in-document link jumps here, a URL opens in a
 * new page. Any other click names a source location for the editor.
 */
function onClick(event: MouseEvent): void {
  const link = linkAt(event)
  if (link) {
    const jump = jumpTarget(link)
    if (jump) {
      // The viewer is a document of its own: a link may never navigate it away.
      event.preventDefault()
      navigation.reveal(jump)
    } else {
      openLink(event, link)
    }
    return
  }
  if (!channel?.ready) return
  const point = pagePointAt(event)
  if (point) void channel.send({ docToSource: point })
}

function linkAt(event: MouseEvent): SVGElement | null {
  const target = event.target
  return target instanceof Element ? target.closest<SVGElement>("svg.ht-page a") : null
}

/**
 * The page point a link jumps to, i.e. its `data-ht-link` tag.
 */
function jumpTarget(link: SVGElement): PagePoint | null {
  const tag = link.getAttribute("data-ht-link")
  if (tag === null) return null
  const [page, x, y] = tag.split(",").map(Number)
  if (page === undefined || x === undefined || y === undefined) return null
  if (!Number.isFinite(page) || !Number.isFinite(x) || !Number.isFinite(y)) return null
  return { page, x, y }
}

/**
 * A URL link carries `target="_blank"`, which a browser opens by itself.
 */
function openLink(event: MouseEvent, link: SVGElement): void {
  if (window.parent === window) return
  event.preventDefault()
  const href = link.getAttribute("href")
  if (href) window.parent.postMessage({ type: "openExternal", url: href }, "*")
}

/**
 * A resize changes the fit-to-width page width the zoom is relative to.
 */
function onResize(): void {
  navigation.refit()
}

onMounted(() => {
  channel = new PreviewChannel(props.transport, {
    onDelta: applyDelta,
    onScrollTo: (target) => navigation.reveal(target),
  })
  window.addEventListener("wheel", navigation.onWheel, { passive: false })
  window.addEventListener("resize", onResize)
})
onBeforeUnmount(() => {
  window.removeEventListener("wheel", navigation.onWheel)
  window.removeEventListener("resize", onResize)
  navigation.dispose()
  channel?.close()
})

defineExpose({ status })
</script>

<template>
  <div class="ht-viewer" @click="onClick">
    <div class="ht-status">
      {{ status }}
    </div>
    <div ref="stage" class="ht-stage">
      <div
        ref="pageContainer"
        class="ht-pages"
        :data-background="settings.background"
        :data-invert="settings.invert ? 'true' : undefined"
      >
        <svg class="ht-res-host" width="0" height="0" aria-hidden="true">
          <defs ref="defs" class="ht-res" />
        </svg>
      </div>
    </div>
  </div>
</template>

<style scoped>
.ht-stage {
  width: max-content;
  min-width: 100%;
  min-height: 100vh;
  overflow: clip;
  background: var(--ht-bg, #525659);
}
.ht-pages {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 16px;
  box-sizing: border-box;
  width: max-content;
  margin-inline: auto;
  transform-origin: top center;
}
/* Out of flow: the resource host must not take a slot in the page column. */
.ht-res-host {
  position: absolute;
}
.ht-pages :deep(.ht-page) {
  width: 100%;
  height: auto;
  flex-shrink: 0;
}
.ht-pages :deep(.ht-page a) {
  cursor: pointer;
}
.ht-pages[data-background="white"] :deep(.ht-page) {
  background-color: #fff;
}
.ht-pages[data-background="transparent"] :deep(.ht-page) {
  background-color: transparent;
}
.ht-pages[data-background="checkerboard"] :deep(.ht-page) {
  background-color: #fff;
  background-image:
    linear-gradient(45deg, #d4d4d4 25%, transparent 25%, transparent 75%, #d4d4d4 75%),
    linear-gradient(45deg, #d4d4d4 25%, transparent 25%, transparent 75%, #d4d4d4 75%);
  background-position:
    0 0,
    16px 16px;
  background-size: 32px 32px;
}
/* The page inverts, but keeps its hues */
.ht-pages[data-invert="true"] :deep(.ht-page) {
  filter: invert(1) hue-rotate(180deg);
}
/* Hover image to restore color. */
.ht-pages[data-invert="true"] :deep(.ht-page image) {
  filter: invert(0) hue-rotate(0deg);
  transition: filter 0.1s ease-in-out;
}
.ht-pages[data-invert="true"] :deep(.ht-page image:hover) {
  filter: invert(1) hue-rotate(180deg);
}
.ht-pages :deep(.ht-jump-highlight) {
  fill: rgb(56 139 253 / 15%);
  stroke: #388bfd;
  stroke-width: 2;
  vector-effect: non-scaling-stroke;
  pointer-events: none;
  animation: ht-jump-pulse 1.2s ease-out both;
}
@media (prefers-reduced-motion: reduce) {
  .ht-pages :deep(.ht-jump-highlight) {
    animation: none;
  }
}
@keyframes ht-jump-pulse {
  from {
    r: 6px;
    fill-opacity: 1;
    stroke-opacity: 1;
  }
  to {
    r: 24px;
    fill-opacity: 0;
    stroke-opacity: 0;
  }
}
.ht-status {
  position: fixed;
  z-index: 1;
  user-select: none;
  top: 8px;
  right: 12px;
  font:
    12px ui-monospace,
    monospace;
  color: #9fd0ff;
  background: rgb(0 0 0 / 50%);
  padding: 2px 8px;
  border-radius: 4px;
}
</style>
