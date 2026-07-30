// @ts-check
import js from "@eslint/js";
import tseslint from "typescript-eslint";

export default tseslint.config(
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    rules: {
      // Enforce explicit return types on public API surface; allow inference
      // inside callbacks and test helpers.
      "@typescript-eslint/explicit-module-boundary-types": "warn",
      // Ban implicit `any` — keeps the strict tsconfig guarantees at lint time.
      "@typescript-eslint/no-explicit-any": "warn",
      // Unused variables are always bugs or dead code.
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_" },
      ],
    },
  },
  {
    // Relax some rules in test files to reduce boilerplate.
    files: ["src/**/*.test.ts"],
    rules: {
      "@typescript-eslint/explicit-module-boundary-types": "off",
      "@typescript-eslint/no-explicit-any": "off",
    },
  },
  {
    ignores: ["dist/**", "generated/**", "node_modules/**"],
  }
);
