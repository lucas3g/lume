// Destaques da resposta do agente: blocos de citação marcados ("> [!ENTREGUE] Título") que o chat
// mostra como cartões. O prompt das sessões pede essas marcas (ver CALLOUT_GUIDE no backend).

import { marked, type Token, type Tokens } from "marked";

export type CalloutKind = "ok" | "act" | "note" | "bad";

const KINDS: Record<string, CalloutKind | "next"> = {
  ENTREGUE: "ok",
  ACAO: "act",
  "AÇÃO": "act",
  FORA: "note",
  ERRO: "bad",
  PROXIMOS: "next",
  "PRÓXIMOS": "next",
};

export const KICKERS: Record<CalloutKind, string> = { ok: "Entregue", act: "Ação sua", note: "Ficou fora", bad: "Erro" };

const MARK = /^\s*\[!([A-Za-zÇçÃãÓó]+)\][ \t]*([^\n]*)(?:\n|$)/;

export interface Callout {
  kind: CalloutKind | "next";
  title: string;
  /** Markdown do corpo, sem a linha da marca. */
  body: string;
}

/** O texto de um bloco de citação é um destaque? */
export function calloutOf(quote: string): Callout | null {
  const m = quote.match(MARK);
  const kind = m ? KINDS[m[1].toUpperCase()] : undefined;
  if (!m || !kind) return null;
  return { kind, title: m[2].trim(), body: quote.slice(m[0].length) };
}

const cache = new Map<string, Callout[]>();

/** Destaques de uma resposta, na ordem em que aparecem. */
export function calloutsIn(markdown: string): Callout[] {
  if (!markdown.includes("[!")) return [];
  const hit = cache.get(markdown);
  if (hit) return hit;
  const found = marked
    .lexer(markdown)
    .filter((t): t is Tokens.Blockquote => t.type === "blockquote")
    .map((t) => calloutOf(t.text))
    .filter((c): c is Callout => c !== null);
  if (cache.size > 400) cache.clear();
  cache.set(markdown, found);
  return found;
}

/** Itens das listas de primeiro nível do corpo: as ações de um [!ACAO], as sugestões de um [!PROXIMOS]. */
export function itemsOf(callout: Callout): string[] {
  return marked
    .lexer(callout.body)
    .filter((t): t is Tokens.List => t.type === "list")
    .flatMap((list) => list.items.map((item) => item.text.replace(/^\[[ xX]\]\s*/, "").trim()));
}

/** Ações do usuário numa resposta, com a posição que o cartão usa para guardar o que foi feito. */
export function actionsIn(markdown: string): string[] {
  return calloutsIn(markdown)
    .filter((c) => c.kind === "act")
    .flatMap(itemsOf);
}

/** Sugestões de próximo pedido de uma resposta. */
export function suggestionsIn(markdown: string): string[] {
  return calloutsIn(markdown)
    .filter((c) => c.kind === "next")
    .flatMap(itemsOf)
    .map((s) => s.replace(/[*_`]/g, ""))
    .filter((s, i, all) => s && all.indexOf(s) === i)
    .slice(0, 3);
}

const escape = (text: string) => text.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);
const inline = (text: string) => marked.parseInline(text, { async: false }) as string;

const ICONS: Record<CalloutKind, string> = {
  ok: '<path d="M5 12.5l4.5 4.5L19 7.5"/>',
  act: '<path d="M8 13V5.5a1.5 1.5 0 0 1 3 0V12M11 11V4.5a1.5 1.5 0 0 1 3 0V12M14 11V6.5a1.5 1.5 0 0 1 3 0V14a7 7 0 0 1-7 7 6 6 0 0 1-5-3l-2.5-4.5a1.5 1.5 0 0 1 2.6-1.5L8 15"/>',
  note: '<circle cx="12" cy="12" r="9"/><path d="M12 11v5M12 8h.01"/>',
  bad: '<path d="M12 3l10 18H2zM12 10v4M12 17.5v.5"/>',
};

const LABELED = /^\*\*([^*]+?):?\*\*:?\s*([\s\S]+)$/;

/** Lista em que todo item é "**Rótulo:** valor": vira blocos de fatos. */
function tiles(list: Tokens.List): string | null {
  const pairs = list.items.map((item) => item.text.trim().match(LABELED));
  if (!pairs.length || pairs.some((p) => !p)) return null;
  const cells = pairs.map((p) => `<div class="tile"><span class="tl">${escape(p![1].trim())}</span><span class="tv">${inline(p![2].trim())}</span></div>`);
  return `<div class="tiles">${cells.join("")}</div>`;
}

/** HTML do cartão; `parse` converte os blocos do corpo com o renderizador do Markdown. */
export function renderCallout(callout: Callout, parse: (tokens: Token[]) => string): string {
  if (callout.kind === "next") return "";
  const kind = callout.kind;
  const tokens = marked.lexer(callout.body);
  let body = "";
  let run: Token[] = [];
  const flush = () => {
    if (run.length) body += parse(run);
    run = [];
  };
  for (const token of tokens) {
    const asTiles = kind === "ok" && token.type === "list" ? tiles(token as Tokens.List) : null;
    if (asTiles) {
      flush();
      body += asTiles;
    } else run.push(token);
  }
  flush();
  const title = callout.title ? `<span class="co-t">${inline(callout.title)}</span>` : "";
  const head = `<span class="co-ic"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${ICONS[kind]}</svg></span><span class="co-tx"><span class="co-k">${KICKERS[kind]}</span>${title}</span>`;
  if (kind === "note") {
    const chev = '<svg class="co-chev" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M6 9l6 6 6-6"/></svg>';
    return `<details class="callout note" data-callout="note"><summary class="co-head">${head}${chev}</summary><div class="co-body">${body}</div></details>`;
  }
  return `<section class="callout ${kind}" data-callout="${kind}"><div class="co-head">${head}</div><div class="co-body">${body}</div></section>`;
}
