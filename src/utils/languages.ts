export interface Language {
  id: string;
  name: string;
  color: string;
}

export const LANGUAGES: Language[] = [
  { id: "plaintext", name: "Plain Text", color: "#6b7280" },
  { id: "javascript", name: "JavaScript", color: "#ca8a04" },
  { id: "typescript", name: "TypeScript", color: "#2563eb" },
  { id: "jsx", name: "JSX", color: "#0891b2" },
  { id: "tsx", name: "TSX", color: "#0e7490" },
  { id: "python", name: "Python", color: "#4338ca" },
  { id: "rust", name: "Rust", color: "#c2410c" },
  { id: "go", name: "Go", color: "#0284c7" },
  { id: "java", name: "Java", color: "#dc2626" },
  { id: "cpp", name: "C++", color: "#be185d" },
  { id: "c", name: "C", color: "#475569" },
  { id: "csharp", name: "C#", color: "#15803d" },
  { id: "php", name: "PHP", color: "#7e22ce" },
  { id: "ruby", name: "Ruby", color: "#be123c" },
  { id: "swift", name: "Swift", color: "#ea580c" },
  { id: "kotlin", name: "Kotlin", color: "#a21caf" },
  { id: "sql", name: "SQL", color: "#0f766e" },
  { id: "html", name: "HTML", color: "#c2410c" },
  { id: "css", name: "CSS", color: "#7c3aed" },
  { id: "json", name: "JSON", color: "#15803d" },
  { id: "yaml", name: "YAML", color: "#b91c1c" },
  { id: "xml", name: "XML", color: "#1e40af" },
  { id: "markdown", name: "Markdown", color: "#1d4ed8" },
  { id: "bash", name: "Bash", color: "#166534" },
  { id: "dockerfile", name: "Dockerfile", color: "#0369a1" },
  { id: "toml", name: "TOML", color: "#b45309" },
  { id: "lua", name: "Lua", color: "#4338ca" },
  { id: "r", name: "R", color: "#0284c7" },
  { id: "scala", name: "Scala", color: "#b91c1c" },
  { id: "elixir", name: "Elixir", color: "#6d28d9" },
];

export const getLang = (id: string): Language =>
  LANGUAGES.find((l) => l.id === id) ?? LANGUAGES[0];
