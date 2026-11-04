// What a session paints when the host does not say otherwise.

import type { PreviewSettings } from "./generated/index.js"

/**
 * The defaults every consumer starts from.
 *
 * Mirrors `PreviewSettings::default()` in `holotyp-preview-protocol`; keep the two in
 * step.
 */
export const DEFAULT_SETTINGS: Readonly<PreviewSettings> = {
  invert: false,
  background: "white",
}
