// Artifacts: blocos de código visuais (HTML ou SVG) que ganham prévia ao lado da conversa.

import { marked, type Token } from "marked";

export interface Artifact {
  kind: "html" | "svg";
  title: string;
  code: string;
}

const KIND_BY_LANG: Record<string, Artifact["kind"]> = { html: "html", svg: "svg" };
const DEFAULT_TITLE: Record<Artifact["kind"], string> = { html: "Página", svg: "Imagem" };

function titleOf(kind: Artifact["kind"], code: string): string {
  const match = code.match(/<title[^>]*>([^<]+)<\/title>/i) ?? code.match(/<h1[^>]*>([^<]+)<\/h1>/i);
  return match?.[1].trim() || DEFAULT_TITLE[kind];
}

/** Blocos ```html e ```svg completos de uma resposta. */
export function extractArtifacts(text: string): Artifact[] {
  const found: Artifact[] = [];
  const visit = (tokens: Token[]) => {
    for (const token of tokens) {
      if (token.type === "code") {
        const kind = KIND_BY_LANG[(token.lang ?? "").trim().toLowerCase()];
        if (kind && token.text.trim()) found.push({ kind, code: token.text, title: titleOf(kind, token.text) });
      } else if ("tokens" in token && token.tokens) {
        visit(token.tokens);
      }
    }
  };
  visit(marked.lexer(text));
  return found;
}

/** Versões de um artifact: os de mesmo tipo e título, na ordem em que apareceram na conversa. */
export function versionsOf(artifact: Artifact, all: Artifact[]): Artifact[] {
  const same = all.filter((a) => a.kind === artifact.kind && a.title === artifact.title);
  return same.length ? same : [artifact];
}

export function fileNameFor(artifact: Artifact): string {
  const base = artifact.title.toLowerCase().normalize("NFD").replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
  return `${base || "artifact"}.${artifact.kind}`;
}

/** Documento exibido na prévia isolada. */
export function previewDocument(artifact: Artifact): string {
  if (artifact.kind === "html") return artifact.code;
  return `<!doctype html><meta charset="utf-8"><body style="margin:0;display:grid;place-items:center;min-height:100vh;background:#fff">${artifact.code}</body>`;
}
