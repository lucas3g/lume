// Apelidos dos conectores da conta claude.ai: o que o usuário procura pelo nome do produto.

import type { AccountConnector } from "./types";

const ALIASES: Record<string, string[]> = {
  atlassian: ["jira", "confluence", "bitbucket", "tickets", "issues"],
  gmail: ["email", "e-mail", "mail", "correio"],
  "google calendar": ["agenda", "calendário", "calendario", "reunião", "reuniao"],
  "google sheets": ["planilha", "sheets", "excel"],
  "google docs": ["documento", "docs", "word"],
  "google drive": ["arquivos", "drive", "nuvem"],
  "google slides": ["apresentação", "apresentacao", "slides", "powerpoint"],
  slack: ["chat", "mensagens", "canal"],
  notion: ["notas", "wiki", "páginas", "paginas"],
  linear: ["issues", "tarefas", "bugs"],
  asana: ["tarefas", "projetos"],
  "monday.com": ["monday", "tarefas", "quadro"],
  hubspot: ["crm", "vendas", "leads"],
  intercom: ["suporte", "atendimento", "chat"],
  figma: ["design", "protótipo", "prototipo", "ui"],
  canva: ["design", "arte", "post"],
  box: ["arquivos", "nuvem"],
  runway: ["vídeo", "video", "imagem"],
};

/** O conector da conta atende à busca pelo nome ou por um produto que ele cobre. */
export function matchesQuery(c: AccountConnector, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  const name = c.display_name.toLowerCase();
  if (name.includes(q)) return true;
  const aliases = ALIASES[name] ?? [];
  return aliases.some((alias) => alias.includes(q) || q.includes(alias));
}

/** Produtos cobertos pelo conector, para mostrar no cartão. */
export function coveredProducts(c: AccountConnector): string[] {
  return (ALIASES[c.display_name.toLowerCase()] ?? []).filter((a) => /^[a-z]/.test(a) && !/[áãéêíóôúç]/.test(a)).slice(0, 3);
}
