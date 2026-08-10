import { describe, expect, it } from "vitest";
import { HighlightStyle } from "@codemirror/language";
import { tags as t } from "@lezer/highlight";
import {
  getSyntaxHighlightRanges,
  normalizeLanguage,
} from "../components/syntaxHighlight";

const highlightStyle = HighlightStyle.define([
  { tag: t.keyword, class: "test-keyword" },
  { tag: t.string, class: "test-string" },
  { tag: t.comment, class: "test-comment" },
  { tag: t.propertyName, class: "test-property" },
]);

function highlightedText(
  content: string,
  language: string,
  className: string,
): string[] {
  return getSyntaxHighlightRanges(content, language, highlightStyle)
    .filter((range) => range.className.includes(className))
    .map((range) => content.slice(range.from, range.to));
}

describe("shared CodeMirror syntax highlight ranges", () => {
  it("uses parser-backed ranges across multiline and embedded HTML content", () => {
    const content = [
      "<script>",
      "const message = 'hello';",
      "</script>",
      "<!-- note -->",
    ].join("\n");

    expect(highlightedText(content, "html", "test-keyword")).toContain("const");
    expect(highlightedText(content, "html", "test-string")).toContain(
      "'hello'",
    );
    expect(highlightedText(content, "html", "test-comment")).toContain(
      "<!-- note -->",
    );
  });

  it("uses the configured StreamLanguage lexical highlighter", () => {
    const content = "class Vault\n  # comment\n  def save\n  end\nend";

    expect(highlightedText(content, "ruby", "test-keyword")).toEqual(
      expect.arrayContaining(["class", "def", "end"]),
    );
    expect(highlightedText(content, "ruby", "test-comment")).toContain(
      "# comment",
    );
  });

  it("keeps plaintext unhighlighted and normalizes invalid language IDs", () => {
    expect(
      getSyntaxHighlightRanges(
        "const visible = true",
        "plaintext",
        highlightStyle,
      ),
    ).toEqual([]);
    expect(normalizeLanguage("not-a-language")).toBe("plaintext");
  });
});
