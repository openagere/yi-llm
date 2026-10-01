import js from "@eslint/js";
import reactHooks from "eslint-plugin-react-hooks";
import globals from "globals";
import tseslint from "typescript-eslint";

export default tseslint.config(
  { ignores: ["dist", "node_modules", "src-tauri", "test-results", "playwright-report"] },
  {
    files: ["src/**/*.{ts,tsx}"],
    extends: [js.configs.recommended, ...tseslint.configs.recommended, reactHooks.configs.flat.recommended],
    languageOptions: { globals: globals.browser },
    rules: {
      "@typescript-eslint/consistent-type-imports": ["error", { prefer: "type-imports" }],
      "@typescript-eslint/no-unused-vars": ["error", { argsIgnorePattern: "^_", varsIgnorePattern: "^_" }],
    },
  },
  {
    files: ["src/features/**/*.{ts,tsx}"],
    rules: {
      "no-restricted-imports": ["error", {
        patterns: [{
          group: ["@/features/*/*", "@/features/*/*/**"],
          message: "跨功能只能从对方的 index.ts 引用。",
        }],
      }],
    },
  },
  {
    files: ["src/features/models/**/*.{ts,tsx}"],
    rules: {
      "no-restricted-imports": ["error", {
        patterns: [
          { group: ["@/features/providers", "@/features/providers/**"], message: "models 不能依赖 providers。" },
          { group: ["@/features/*/*", "@/features/*/*/**"], message: "跨功能只能从对方的 index.ts 引用。" },
        ],
      }],
    },
  },
  {
    files: ["tests/**/*.ts", "scripts/**/*.mjs", "*.config.{js,mjs,ts}"],
    extends: [js.configs.recommended, ...tseslint.configs.recommended],
    languageOptions: { globals: { ...globals.node, ...globals.browser } },
  },
);
