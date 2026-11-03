const SVG_NS = "http://www.w3.org/2000/svg"

/**
 * One element in the SVG namespace.
 */
export function svgElement<K extends keyof SVGElementTagNameMap>(
  name: K,
): SVGElementTagNameMap[K] {
  return document.createElementNS(SVG_NS, name)
}

/**
 * One `<g>` holding an item body, which the renderer drew at the page origin.
 */
export function bodyNode(body: string): SVGGElement {
  const node = svgElement("g")
  node.innerHTML = body
  return node
}
