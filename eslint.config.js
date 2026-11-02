import { dirname, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import globals from "globals"
import js from "@eslint/js"
import pluginVue from "eslint-plugin-vue"
import pluginOxfmt from "eslint-plugin-oxfmt"
import skipFormatting from "eslint-config-prettier/flat"
import { withVueTs, vueTsConfigs } from "@vue/eslint-config-typescript"

export default withVueTs(
  {
    name: "app/files-to-ignore",
    ignores: ["**/dist/**", "./packages/preview-protocol/src/generated"],
  },
  js.configs.recommended,
  pluginVue.configs["flat/recommended"],
  vueTsConfigs.recommended,
  {
    name: "app/language-options",
    languageOptions: {
      sourceType: "module",
      globals: {
        ...globals.browser,
      },
    },
    rules: {},
  },
  {
    name: "app/oxfmt",
    ...pluginOxfmt.configs.recommendedWithoutParser,
    files: ["**/*.{js,mjs,cjs,ts,mts,vue}"],
    rules: {
      "oxfmt/oxfmt": [
        "error",
        { configPath: resolve(dirname(fileURLToPath(import.meta.url)), ".oxfmtrc.json") },
      ],
    },
  },

  // Formatting belongs to oxfmt alone
  skipFormatting,
)
