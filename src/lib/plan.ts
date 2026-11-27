// O plano que o agente apresenta (Markdown) organizado para o cartão de aprovação:
// título, seções, passos numerados e arquivos citados.

import { marked, type Tokens } from "marked";

export interface PlanStep {
  /** Posição no plano inteiro (0, 1, 2…), para os comentários. */
  index: number;
  text: string;
}

export type PlanPart = { kind: "md"; raw: string } | { kind: "steps"; items: PlanStep[] };

export interface PlanSection {
  heading: string | null;
  parts: PlanPart[];
}

export interface ParsedPlan {
  title: string;
  sections: PlanSection[];
  steps: PlanStep[];
  files: string[];
}

const CODE_FILE = /\.(ts|tsx|js|jsx|mjs|svelte|vue|rs|dart|py|swift|kt|java|cs|go|rb|php|sql|json|ya?ml|toml|md|css|scss|html|sh)$/i;

export const plainText = (text: string) => text.replace(/[*_`]/g, "").trim();

/** Título do passo: a primeira linha, sem marcação. */
export const stepTitle = (text: string) => plainText(text.split("\n")[0]);

export function parsePlan(markdown: string): ParsedPlan {
  let title = "";
  const sections: PlanSection[] = [];
  const steps: PlanStep[] = [];
  let current: PlanSection = { heading: null, parts: [] };
  const close = () => {
    if (current.heading !== null || current.parts.some((p) => p.kind === "steps" || p.raw.trim())) sections.push(current);
  };
  for (const token of marked.lexer(markdown)) {
    if (token.type === "heading") {
      const heading = token as Tokens.Heading;
      if (heading.depth === 1 && !title) {
        title = plainText(heading.text);
        continue;
      }
      close();
      current = { heading: plainText(heading.text), parts: [] };
    } else if (token.type === "list" && (token as Tokens.List).ordered) {
      const items = (token as Tokens.List).items.map((item) => ({ index: steps.length, text: item.text }) as PlanStep);
      items.forEach((item, i) => (item.index = steps.length + i));
      steps.push(...items);
      current.parts.push({ kind: "steps", items });
    } else {
      const last = current.parts.at(-1);
      if (last?.kind === "md") last.raw += token.raw;
      else current.parts.push({ kind: "md", raw: token.raw });
    }
  }
  close();
  const files = [...new Set([...markdown.matchAll(/`([^`\s]+\.[A-Za-z0-9]{1,8})`/g)].map((m) => m[1]))].filter(
    (f) => CODE_FILE.test(f) || f.includes("/"),
  );
  return { title: title || "Plano", sections, steps, files };
}
