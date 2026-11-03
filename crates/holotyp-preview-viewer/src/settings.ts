import {
  DEFAULT_SETTINGS,
  type PreviewSettings,
} from "@asobisoft/holotyp-preview-protocol"

/**
 * The settings of this session.
 *
 * The host replaces the `ht-settings` slot in `index.html` before serving it; without a
 * slot (the vite dev server) the defaults apply.
 */
export function readSettings(): PreviewSettings {
  const settings = readSlot()
  return import.meta.env.DEV ? devOverride(settings) : settings
}

/**
 * What the host filled into the page, or the defaults.
 */
function readSlot(): PreviewSettings {
  const slot = document.getElementById("ht-settings")
  if (!slot?.textContent) return DEFAULT_SETTINGS
  try {
    const raw: unknown = JSON.parse(slot.textContent)
    if (typeof raw !== "object" || raw === null) return DEFAULT_SETTINGS
    const { invert, background } = raw as Partial<PreviewSettings>
    return {
      invert: invert === true,
      background: background ?? DEFAULT_SETTINGS.background,
    }
  } catch {
    return DEFAULT_SETTINGS
  }
}

function devOverride(settings: PreviewSettings): PreviewSettings {
  // eslint-disable-next-line no-constant-condition
  return false
    ? {
        invert: false,
        background: "white",
      }
    : settings
}
