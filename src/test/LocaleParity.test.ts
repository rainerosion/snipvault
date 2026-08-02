import { describe, expect, it } from "vitest";
import en from "../i18n/locales/en.json";
import zh from "../i18n/locales/zh.json";

type TranslationTree = Record<string, unknown>;

function flattenKeys(tree: TranslationTree, prefix = ""): string[] {
  return Object.entries(tree).flatMap(([key, value]) => {
    const path = prefix ? `${prefix}.${key}` : key;
    if (value && typeof value === "object" && !Array.isArray(value)) {
      return flattenKeys(value as TranslationTree, path);
    }
    return [path];
  });
}

describe("locale resources", () => {
  it("keeps Chinese and English translation keys synchronized", () => {
    const enKeys = flattenKeys(en).sort();
    const zhKeys = flattenKeys(zh).sort();

    expect(zhKeys).toEqual(enKeys);
  });
});
