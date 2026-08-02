import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { DEFAULT_SETTINGS } from "./settingsFixtures";

const read = (path: string) =>
  readFileSync(resolve(process.cwd(), path), "utf8");

describe("desktop security configuration", () => {
  it("keeps the SettingsView contract redacted", () => {
    const serialized = JSON.stringify(DEFAULT_SETTINGS);
    expect(serialized).not.toContain("webdav_password");
    expect(DEFAULT_SETTINGS.webdav_secret_configured).toBe(true);
  });

  it("uses Vite-managed boot assets without inline executable content", () => {
    const html = read("index.html");
    expect(html).toContain('src="/src/boot.ts"');
    expect(html).not.toMatch(/<style(?:\s|>)/i);
    expect(html).not.toMatch(/<script(?![^>]*\bsrc=)[^>]*>/i);
  });

  it("disables global Tauri, enforces CSP, and removes broad Shell scope", () => {
    const config = JSON.parse(read("src-tauri/tauri.conf.json"));
    const capability = JSON.parse(read("src-tauri/capabilities/default.json"));
    const packageJson = JSON.parse(read("package.json"));
    const cargoToml = read("src-tauri/Cargo.toml");

    expect(config.app.withGlobalTauri).toBe(false);
    expect(config.app.security.csp).toContain("script-src 'self'");
    expect(config.app.security.csp).toContain(
      "connect-src ipc: http://ipc.localhost",
    );
    expect(config.app.security.csp).not.toContain("unsafe-eval");
    expect(config.plugins?.shell).toBeUndefined();
    expect(capability.permissions).not.toContain("shell:allow-open");
    expect(capability.permissions).not.toContain("core:window:allow-create");
    expect(capability.permissions).not.toContain("core:window:allow-set-title");
    expect(
      packageJson.dependencies["@tauri-apps/plugin-shell"],
    ).toBeUndefined();
    expect(cargoToml).not.toContain("tauri-plugin-shell");
  });
});
