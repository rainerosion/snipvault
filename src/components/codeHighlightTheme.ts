import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { tags as t } from "@lezer/highlight";
import { githubDarkStyle, githubLightStyle } from "@uiw/codemirror-theme-github";
import { StyleModule } from "style-mod";

export const DARK_SYNTAX_COLORS = {
  keyword: "#ff7b72",
  name: "#ffa657",
  functionName: "#d2a8ff",
  type: "#79c0ff",
  string: "#a5d6ff",
  number: "#79c0ff",
  comment: "#8b949e",
  punctuation: "#ff7b72",
  plain: "#c9d1d9",
};

export const LIGHT_SYNTAX_COLORS = {
  keyword: "#cf222e",
  name: "#953800",
  functionName: "#8250df",
  type: "#0550ae",
  string: "#0a3069",
  number: "#0550ae",
  comment: "#6e7781",
  punctuation: "#cf222e",
  plain: "#24292f",
};

export const darkHighlightStyle = HighlightStyle.define([
  ...githubDarkStyle,
  { tag: t.keyword, color: DARK_SYNTAX_COLORS.keyword },
  { tag: [t.name, t.deleted, t.character, t.propertyName, t.macroName], color: DARK_SYNTAX_COLORS.name },
  { tag: [t.function(t.variableName), t.function(t.propertyName)], color: DARK_SYNTAX_COLORS.functionName },
  { tag: [t.labelName], color: "#7ee787" },
  { tag: [t.color, t.constant(t.name), t.standard(t.name)], color: DARK_SYNTAX_COLORS.name },
  { tag: [t.definition(t.name), t.separator], color: DARK_SYNTAX_COLORS.plain },
  { tag: [t.typeName, t.className, t.number, t.changed, t.annotation, t.modifier, t.self, t.namespace], color: DARK_SYNTAX_COLORS.type },
  { tag: [t.operator, t.operatorKeyword, t.url, t.escape, t.regexp, t.link, t.special(t.string)], color: DARK_SYNTAX_COLORS.punctuation },
  { tag: [t.meta, t.comment], color: DARK_SYNTAX_COLORS.comment, fontStyle: "italic" },
  { tag: t.strong, fontWeight: "bold" },
  { tag: t.emphasis, fontStyle: "italic" },
  { tag: t.strikethrough, textDecoration: "line-through" },
  { tag: t.link, color: DARK_SYNTAX_COLORS.string, textDecoration: "underline" },
  { tag: t.heading, fontWeight: "bold", color: DARK_SYNTAX_COLORS.type },
  { tag: [t.atom, t.bool, t.special(t.variableName)], color: DARK_SYNTAX_COLORS.keyword },
  { tag: [t.processingInstruction, t.string, t.inserted], color: DARK_SYNTAX_COLORS.string },
  { tag: t.number, color: DARK_SYNTAX_COLORS.number },
  { tag: t.invalid, color: DARK_SYNTAX_COLORS.keyword },
]);

export const lightHighlightStyle = HighlightStyle.define([
  ...githubLightStyle,
  { tag: t.keyword, color: LIGHT_SYNTAX_COLORS.keyword },
  { tag: [t.name, t.deleted, t.character, t.propertyName, t.macroName], color: LIGHT_SYNTAX_COLORS.name },
  { tag: [t.function(t.variableName), t.function(t.propertyName)], color: LIGHT_SYNTAX_COLORS.functionName },
  { tag: [t.labelName], color: "#116329" },
  { tag: [t.color, t.constant(t.name), t.standard(t.name)], color: LIGHT_SYNTAX_COLORS.name },
  { tag: [t.definition(t.name), t.separator], color: LIGHT_SYNTAX_COLORS.plain },
  { tag: [t.typeName, t.className, t.number, t.changed, t.annotation, t.modifier, t.self, t.namespace], color: LIGHT_SYNTAX_COLORS.type },
  { tag: [t.operator, t.operatorKeyword, t.url, t.escape, t.regexp, t.link, t.special(t.string)], color: LIGHT_SYNTAX_COLORS.punctuation },
  { tag: [t.meta, t.comment], color: LIGHT_SYNTAX_COLORS.comment, fontStyle: "italic" },
  { tag: t.strong, fontWeight: "bold" },
  { tag: t.emphasis, fontStyle: "italic" },
  { tag: t.strikethrough, textDecoration: "line-through" },
  { tag: t.link, color: LIGHT_SYNTAX_COLORS.string, textDecoration: "underline" },
  { tag: t.heading, fontWeight: "bold", color: LIGHT_SYNTAX_COLORS.type },
  { tag: [t.atom, t.bool, t.special(t.variableName)], color: LIGHT_SYNTAX_COLORS.keyword },
  { tag: [t.processingInstruction, t.string, t.inserted], color: LIGHT_SYNTAX_COLORS.string },
  { tag: t.number, color: LIGHT_SYNTAX_COLORS.number },
  { tag: t.invalid, color: LIGHT_SYNTAX_COLORS.keyword },
]);

const darkHighlight = syntaxHighlighting(darkHighlightStyle);
const lightHighlight = syntaxHighlighting(lightHighlightStyle);

export function getCodeHighlightStyle(theme: "dark" | "light") {
  return theme === "dark" ? darkHighlightStyle : lightHighlightStyle;
}

export function getCodeHighlightExtension(theme: "dark" | "light") {
  return theme === "dark" ? darkHighlight : lightHighlight;
}

export function mountCodeHighlightStyle(theme: "dark" | "light") {
  if (typeof document === "undefined") return;
  const module = getCodeHighlightStyle(theme).module;
  if (module) StyleModule.mount(document, module);
}
