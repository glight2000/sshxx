import js from "@eslint/js";
import eslintConfigPrettier from "eslint-config-prettier";
import svelte from "eslint-plugin-svelte";
import tsPlugin from "@typescript-eslint/eslint-plugin";
import tsParser from "@typescript-eslint/parser";

const sharedRules = {
  "@typescript-eslint/ban-ts-comment": "off",
  "@typescript-eslint/no-empty-function": "off",
  "@typescript-eslint/no-explicit-any": "off",
  "@typescript-eslint/no-inferrable-types": "off",
  "@typescript-eslint/no-non-null-assertion": "off",
  "@typescript-eslint/no-unsafe-function-type": "off",
  "no-constant-condition": "off",
  "no-control-regex": "off",
  "no-empty": "off",
  "no-undef": "off",
  "no-unused-vars": "off",
  "no-useless-assignment": "off",
  "svelte/no-navigation-without-resolve": "off",
  "svelte/no-unused-svelte-ignore": "off",
  "svelte/no-useless-mustaches": "off",
  "svelte/prefer-svelte-reactivity": "off",
  "svelte/require-each-key": "off",
};

export default [
  {
    ignores: [
      "build/**",
      ".svelte-kit/**",
      "src-tauri/**",
      "target/**",
      "clients/electron/dist/**",
      "clients/electron/node_modules/**",
      "clients/godot/.godot/**",
      "clients/godot/addons/**",
    ],
  },
  js.configs.recommended,
  {
    files: ["**/*.{js,mjs,ts}"],
    languageOptions: {
      parser: tsParser,
      parserOptions: { ecmaVersion: "latest", sourceType: "module" },
    },
    plugins: { "@typescript-eslint": tsPlugin },
    rules: {
      ...tsPlugin.configs.recommended.rules,
      ...sharedRules,
    },
  },
  ...svelte.configs["flat/recommended"],
  {
    files: ["**/*.svelte"],
    languageOptions: {
      parserOptions: { parser: tsParser },
    },
    plugins: { "@typescript-eslint": tsPlugin },
    rules: sharedRules,
  },
  eslintConfigPrettier,
];
