<script setup lang="ts">
import PreviewSurface from "./components/PreviewSurface.vue"
import { webSocketTransport } from "@asobisoft/holotyp-preview-protocol"
import { readSettings } from "./settings"

/**
 * In a dev build the viewer is served by vite while the host is a separate process,
 * typically `typst preview --port 8080`.
 */
const DEV_ENDPOINT = "ws://127.0.0.1:8080/ws"

function endpoint(): string {
  if (import.meta.env.DEV) return DEV_ENDPOINT
  const override = new URLSearchParams(location.search).get("ws")
  if (override) return override
  const scheme = location.protocol === "https:" ? "wss:" : "ws:"
  return `${scheme}//${location.host}/ws`
}

const transport = webSocketTransport(endpoint())
const settings = readSettings()
</script>

<template>
  <PreviewSurface :transport="transport" :settings="settings" />
</template>
