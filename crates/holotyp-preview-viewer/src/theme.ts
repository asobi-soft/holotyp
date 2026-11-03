// The page column and the scrollbar follow the host's theme.

export function applyTheme(): void {
  const root = document.documentElement
  const query = new URLSearchParams(location.search)
  const theme = query.get("theme")
  if (theme === "light" || theme === "dark") root.style.colorScheme = theme
  const background = query.get("background")?.trim()
  if (background && CSS.supports("color", background)) {
    root.style.setProperty("--ht-bg", background)
  }
}
