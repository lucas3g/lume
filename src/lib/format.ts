import DOMPurify from "dompurify";
import { marked } from "marked";
import { highlight } from "./highlight";
import type { FileView } from "./types";

marked.setOptions({ gfm: true, breaks: true });
// Blocos de código das respostas saem coloridos conforme a linguagem indicada.
marked.use({
  renderer: {
    code({ text, lang }) {
      const language = (lang ?? "").trim().split(/\s+/)[0].toLowerCase() || null;
      return `<pre><code class="hljs">${highlight(text, language)}</code></pre>\n`;
    },
  },
});

/** Markdown do modelo convertido em HTML seguro para exibição. */
export function renderMarkdown(text: string): string {
  return DOMPurify.sanitize(marked.parse(text, { async: false }));
}

const timeFormat = new Intl.DateTimeFormat("pt-BR", { hour: "2-digit", minute: "2-digit" });
const dateFormat = new Intl.DateTimeFormat("pt-BR", { day: "2-digit", month: "short" });

export function nowSeconds(): number {
  return Math.floor(Date.now() / 1000);
}

export function formatTime(seconds: number): string {
  return timeFormat.format(new Date(seconds * 1000));
}

/**
 * "Hoje, 14:12", "Ontem, 21:40" ou "03 de nov., 09:00". `now` (ms) é a referência de
 * "hoje"; telas que ficam abertas passam um relógio reativo para o rótulo virar à meia-noite.
 */
export function formatWhen(seconds: number, now: number = Date.now()): string {
  const date = new Date(seconds * 1000);
  const startOfToday = new Date(now);
  startOfToday.setHours(0, 0, 0, 0);
  const days = Math.round((startOfToday.getTime() - new Date(date).setHours(0, 0, 0, 0)) / 86_400_000);
  const day = days === 0 ? "Hoje" : days === 1 ? "Ontem" : dateFormat.format(date);
  return `${day}, ${timeFormat.format(date)}`;
}

/** Instante em ISO 8601, para o atributo `datetime` de `<time>`. */
export function isoDate(seconds: number): string {
  return new Date(seconds * 1000).toISOString();
}

const fullFormat = new Intl.DateTimeFormat("pt-BR", { dateStyle: "full", timeStyle: "medium" });

/** Data e hora completas, para a dica ao passar o mouse. */
export function fullDate(seconds: number): string {
  return fullFormat.format(new Date(seconds * 1000));
}

const weekdayFormat = new Intl.DateTimeFormat("pt-BR", { weekday: "short" });

/** Quando um limite zera: "às 13:50" se for hoje, senão "qua., 07:00". */
export function formatReset(seconds: number): string {
  const date = new Date(seconds * 1000);
  const time = timeFormat.format(date);
  return date.toDateString() === new Date().toDateString() ? `às ${time}` : `${weekdayFormat.format(date)}, ${time}`;
}

export function percent(fraction: number | null): number {
  return Math.round(Math.min(Math.max(fraction ?? 0, 0), 1) * 100);
}

export function folderName(path: string | null): string {
  if (!path) return "";
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

/** Resumo de um arquivo aberto: tipo, linhas (se for texto) e tamanho. */
export function describeFile(view: FileView): string {
  const size =
    view.size < 1024 ? `${view.size} B` : `${(view.size / 1024).toLocaleString("pt-BR", { maximumFractionDigits: 1 })} KB`;
  const name = view.path.split(/[\\/]/).pop() ?? view.path;
  const kind = name.includes(".") ? name.split(".").pop()!.toUpperCase() : "Arquivo";
  return view.text === null ? `${kind} · ${size}` : `${kind} · ${view.text.split("\n").length} linhas · ${size}`;
}
