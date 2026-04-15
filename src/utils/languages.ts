export interface Language {
  id: string;
  name: string;
  color: string;
}

export const LANGUAGES: Language[] = [
  { id: "plaintext", name: "Plain Text", color: "#9ca3af" },
  { id: "javascript", name: "JavaScript", color: "#f7df1e" },
  { id: "typescript", name: "TypeScript", color: "#60a5fa" },
  { id: "jsx", name: "JSX", color: "#61dafb" },
  { id: "tsx", name: "TSX", color: "#61dafb" },
  { id: "python", name: "Python", color: "#a5b4fc" },
  { id: "rust", name: "Rust", color: "#f97316" },
  { id: "go", name: "Go", color: "#22d3ee" },
  { id: "java", name: "Java", color: "#f87171" },
  { id: "cpp", name: "C++", color: "#f472b6" },
  { id: "c", name: "C", color: "#94a3b8" },
  { id: "csharp", name: "C#", color: "#4ade80" },
  { id: "php", name: "PHP", color: "#a78bfa" },
  { id: "ruby", name: "Ruby", color: "#fb7185" },
  { id: "swift", name: "Swift", color: "#fb923c" },
  { id: "kotlin", name: "Kotlin", color: "#c084fc" },
  { id: "sql", name: "SQL", color: "#fbbf24" },
  { id: "html", name: "HTML", color: "#fb923c" },
  { id: "css", name: "CSS", color: "#c084fc" },
  { id: "json", name: "JSON", color: "#86efac" },
  { id: "yaml", name: "YAML", color: "#f87171" },
  { id: "xml", name: "XML", color: "#60a5fa" },
  { id: "markdown", name: "Markdown", color: "#93c5fd" },
  { id: "bash", name: "Bash", color: "#86efac" },
  { id: "dockerfile", name: "Dockerfile", color: "#67e8f9" },
  { id: "toml", name: "TOML", color: "#fb923c" },
  { id: "lua", name: "Lua", color: "#818cf8" },
  { id: "r", name: "R", color: "#38bdf8" },
  { id: "scala", name: "Scala", color: "#fb7185" },
  { id: "elixir", name: "Elixir", color: "#c084fc" },
];

export const getLang = (id: string): Language =>
  LANGUAGES.find((l) => l.id === id) ?? LANGUAGES[0];
