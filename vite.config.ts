import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import packageJson from "./package.json";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  define: {
    "import.meta.env.VITE_APP_VERSION": JSON.stringify(packageJson.version),
  },
  build: {
    rollupOptions: {
      output: {
        manualChunks(id) {
          const moduleId = id.replace(/\\/g, "/");
          if (!moduleId.includes("node_modules/")) return undefined;

          if (
            moduleId.includes("/node_modules/react/") ||
            moduleId.includes("/node_modules/react-dom/") ||
            moduleId.includes("/node_modules/scheduler/")
          ) {
            return "react-vendor";
          }

          if (
            moduleId.includes("/node_modules/@codemirror/view/") ||
            moduleId.includes("/node_modules/@codemirror/state/") ||
            moduleId.includes("/node_modules/@codemirror/language/") ||
            moduleId.includes("/node_modules/@lezer/common/") ||
            moduleId.includes("/node_modules/@lezer/highlight/") ||
            moduleId.includes("/node_modules/@lezer/lr/") ||
            moduleId.includes("/node_modules/style-mod/")
          ) {
            return "editor-runtime";
          }

          if (
            moduleId.includes("/node_modules/@codemirror/commands/") ||
            moduleId.includes("/node_modules/@codemirror/autocomplete/") ||
            moduleId.includes("/node_modules/@codemirror/search/") ||
            moduleId.includes("/node_modules/@codemirror/lint/")
          ) {
            return "editor-services";
          }

          if (
            moduleId.includes("/node_modules/@uiw/react-codemirror/") ||
            moduleId.includes("/node_modules/@uiw/codemirror-theme-github/") ||
            moduleId.includes("/node_modules/codemirror/")
          ) {
            return "editor-ui";
          }

          if (
            moduleId.includes("/node_modules/@codemirror/lang-javascript/") ||
            moduleId.includes("/node_modules/@codemirror/lang-html/") ||
            moduleId.includes("/node_modules/@codemirror/lang-css/") ||
            moduleId.includes("/node_modules/@codemirror/lang-xml/") ||
            moduleId.includes("/node_modules/@codemirror/lang-markdown/") ||
            moduleId.includes("/node_modules/@lezer/javascript/") ||
            moduleId.includes("/node_modules/@lezer/html/") ||
            moduleId.includes("/node_modules/@lezer/css/") ||
            moduleId.includes("/node_modules/@lezer/xml/") ||
            moduleId.includes("/node_modules/@lezer/markdown/")
          ) {
            return "editor-lang-web";
          }

          if (
            moduleId.includes("/node_modules/@codemirror/lang-json/") ||
            moduleId.includes("/node_modules/@codemirror/lang-sql/") ||
            moduleId.includes("/node_modules/@codemirror/lang-yaml/") ||
            moduleId.includes("/node_modules/@lezer/json/") ||
            moduleId.includes("/node_modules/@lezer/yaml/")
          ) {
            return "editor-lang-data";
          }

          if (
            moduleId.includes("/node_modules/@codemirror/lang-cpp/") ||
            moduleId.includes("/node_modules/@lezer/cpp/")
          ) {
            return "editor-lang-cpp";
          }

          if (
            moduleId.includes("/node_modules/@codemirror/lang-go/") ||
            moduleId.includes("/node_modules/@lezer/go/")
          ) {
            return "editor-lang-go";
          }

          if (
            moduleId.includes("/node_modules/@codemirror/lang-java/") ||
            moduleId.includes("/node_modules/@lezer/java/")
          ) {
            return "editor-lang-java";
          }

          if (
            moduleId.includes("/node_modules/@codemirror/lang-php/") ||
            moduleId.includes("/node_modules/@lezer/php/")
          ) {
            return "editor-lang-php";
          }

          if (
            moduleId.includes("/node_modules/@codemirror/lang-python/") ||
            moduleId.includes("/node_modules/@lezer/python/")
          ) {
            return "editor-lang-python";
          }

          if (
            moduleId.includes("/node_modules/@codemirror/lang-rust/") ||
            moduleId.includes("/node_modules/@lezer/rust/")
          ) {
            return "editor-lang-rust";
          }

          if (
            moduleId.includes("/node_modules/@replit/codemirror-lang-csharp/")
          ) {
            return "editor-lang-csharp";
          }

          if (
            moduleId.includes("/node_modules/codemirror-lang-elixir/") ||
            moduleId.includes("/node_modules/lezer-elixir/")
          ) {
            return "editor-lang-elixir";
          }

          if (moduleId.includes("/node_modules/@codemirror/legacy-modes/")) {
            return "editor-lang-legacy";
          }

          return undefined;
        },
      },
    },
  },
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
});
