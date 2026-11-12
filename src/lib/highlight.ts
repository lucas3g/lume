// Realce de sintaxe (tema Dracula, definido em app.css) para arquivos e blocos de código.

import hljs from "highlight.js/lib/common";
import dart from "highlight.js/lib/languages/dart";
import dockerfile from "highlight.js/lib/languages/dockerfile";
import gradle from "highlight.js/lib/languages/gradle";
import groovy from "highlight.js/lib/languages/groovy";
import properties from "highlight.js/lib/languages/properties";

hljs.registerLanguage("dart", dart);
hljs.registerLanguage("dockerfile", dockerfile);
hljs.registerLanguage("gradle", gradle);
hljs.registerLanguage("groovy", groovy);
hljs.registerLanguage("properties", properties);

/** Acima disso o texto é mostrado sem cores, para não travar a interface. */
const MAX_HIGHLIGHT_CHARS = 300_000;

const BY_EXTENSION: Record<string, string> = {
  ts: "typescript", tsx: "typescript", mts: "typescript", cts: "typescript",
  js: "javascript", jsx: "javascript", mjs: "javascript", cjs: "javascript",
  rs: "rust", dart: "dart", py: "python", rb: "ruby", go: "go", php: "php", lua: "lua",
  java: "java", kt: "kotlin", kts: "kotlin", swift: "swift", cs: "csharp",
  c: "c", h: "c", cpp: "cpp", cc: "cpp", hpp: "cpp", m: "objectivec", mm: "objectivec",
  json: "json", jsonl: "json", arb: "json", yaml: "yaml", yml: "yaml", toml: "ini", ini: "ini", cfg: "ini", conf: "ini",
  md: "markdown", markdown: "markdown",
  html: "xml", htm: "xml", xml: "xml", svg: "xml", plist: "xml", svelte: "xml", vue: "xml", xsd: "xml",
  css: "css", scss: "scss", less: "less",
  sh: "bash", bash: "bash", zsh: "bash", sql: "sql", graphql: "graphql", gql: "graphql",
  gradle: "gradle", groovy: "groovy", properties: "properties", env: "properties",
  diff: "diff", patch: "diff", mk: "makefile",
};

const BY_FILE_NAME: Record<string, string> = {
  dockerfile: "dockerfile",
  makefile: "makefile",
  podfile: "ruby",
  gemfile: "ruby",
  ".gitignore": "properties",
  ".env": "properties",
};

/** Linguagem de um arquivo pelo nome ou pela extensão; `null` quando desconhecida. */
export function languageOf(path: string): string | null {
  const name = (path.split(/[\\/]/).pop() ?? path).toLowerCase();
  if (BY_FILE_NAME[name]) return BY_FILE_NAME[name];
  const extension = name.includes(".") ? name.split(".").pop()! : "";
  return BY_EXTENSION[extension] ?? null;
}

function escapeHtml(text: string): string {
  return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

/** HTML colorido de um trecho de código; texto simples (escapado) se a linguagem não for conhecida. */
export function highlight(code: string, language: string | null): string {
  if (!language || !hljs.getLanguage(language) || code.length > MAX_HIGHLIGHT_CHARS) return escapeHtml(code);
  return hljs.highlight(code, { language, ignoreIllegals: true }).value;
}

/**
 * Divide o HTML colorido em linhas independentes: marcações que atravessam a
 * quebra de linha (comentários e textos longos) são fechadas e reabertas.
 */
export function splitLines(html: string): string[] {
  const lines: string[] = [];
  const open: string[] = [];
  let current = "";
  let last = 0;
  const token = /<span[^>]*>|<\/span>|\n/g;
  for (let match = token.exec(html); match; match = token.exec(html)) {
    current += html.slice(last, match.index);
    last = token.lastIndex;
    if (match[0] === "\n") {
      lines.push(current + "</span>".repeat(open.length));
      current = open.join("");
    } else {
      if (match[0] === "</span>") open.pop();
      else open.push(match[0]);
      current += match[0];
    }
  }
  lines.push(current + html.slice(last) + "</span>".repeat(open.length));
  return lines;
}

/** Linhas coloridas de um arquivo, prontas para exibir com numeração. */
export function highlightFile(text: string, path: string): string[] {
  return splitLines(highlight(text, languageOf(path)));
}
