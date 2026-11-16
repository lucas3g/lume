// Como apresentar a esteira de bots: estações, estados, rótulos e o log de cada etapa.

import { parseToolUse, summarize } from "./tools";
import { modelLabel } from "./types";
import type { Message, PipelineDetail, Provider, Tier, TierModels, PipelineRole, PipelineStatus, StageRun } from "./types";

export interface Station {
  role: PipelineRole;
  name: string;
  /** Nível de modelo, como aparece no cartão; cada serviço tem o modelo dele para o nível. */
  model: string | null;
  tier: Tier | null;
  perm: string;
  does: string;
  receives: string;
  delivers: string;
  gate?: boolean;
}

export const STATIONS: Station[] = [
  { role: "triage", name: "Triagem", model: "Modelo rápido", tier: "fast", perm: "só leitura",
    does: "Lê a task e aponta o que está vago. Se faltar informação para um plano seguro, a esteira para aqui.",
    receives: "A task: título, descrição e critérios de aceite.", delivers: "O objetivo, os critérios e o que está vago, com o veredito." },
  { role: "planner", name: "Planejador", model: "Modelo forte", tier: "strong", perm: "só leitura",
    does: "Estuda o código e escreve o plano: arquivos, abordagem, riscos e como verificar.",
    receives: "A task e as notas da Triagem.", delivers: "O plano em Markdown." },
  { role: "approval", name: "Sua aprovação", model: null, tier: null, perm: "você decide", gate: true,
    does: "Você lê o plano e aprova ou pede ajustes. Nada é alterado antes disso, e a espera não conta no orçamento.",
    receives: "O plano do Planejador.", delivers: "A sua decisão: aprovar o plano ou pedir ajustes." },
  { role: "implementer", name: "Implementador", model: "Modelo forte", tier: "strong", perm: "edita",
    does: "Executa o plano aprovado na pasta de trabalho. Quando o trabalho volta, corrige só o que foi apontado.",
    receives: "O plano aprovado e, nas voltas, a lista de problemas.", delivers: "As alterações e um resumo arquivo por arquivo." },
  { role: "reviewer", name: "Revisor", model: "Modelo forte", tier: "strong", perm: "só leitura",
    does: "Recebe só o diff, sem ver o raciocínio de quem implementou. Aprova ou devolve com a lista de problemas.",
    receives: "Só o diff das alterações.", delivers: "A aprovação ou a lista de problemas." },
  { role: "tester", name: "Testador", model: "Modelo equilibrado", tier: "balanced", perm: "executa",
    does: "Roda as verificações do projeto (tipos, testes, lint) e relata o resultado. Não corrige código.",
    receives: "A pasta de trabalho com as alterações aprovadas.", delivers: "Cada comando rodado, o resultado e os erros." },
  { role: "reporter", name: "Relator", model: "Modelo rápido", tier: "fast", perm: "Jira",
    does: "Comenta o resultado na task, move para revisão e sugere a mensagem de commit. Não faz commit nem push.",
    receives: "Plano, resumo da implementação, revisão e testes.", delivers: "O comentário no Jira, a mudança de status e a sugestão de commit." },
];

export const TIERS: { tier: Tier; label: string }[] = [
  { tier: "fast", label: "Rápido" },
  { tier: "balanced", label: "Equilibrado" },
  { tier: "strong", label: "Forte" },
];

/** Etapas que o projeto pode desligar; as demais garantem plano aprovado e código revisado. */
export const OPTIONAL_ROLES: PipelineRole[] = ["triage", "tester", "reporter"];

/** Ajuste que você pede ao reabrir uma execução concluída; aparece no registro, não na esteira. */
export const FEEDBACK_STEP: Station = {
  role: "feedback", name: "Seu ajuste", model: null, tier: null, perm: "você pediu", gate: true,
  does: "Depois de testar o que foi entregue, você descreve o que não funcionou e a esteira volta ao Implementador.",
  receives: "A execução concluída.", delivers: "O que precisa mudar.",
};

export const STATION = Object.fromEntries([...STATIONS, FEEDBACK_STEP].map((s) => [s.role, s])) as Record<PipelineRole, Station>;

/** Estado visual de uma estação. */
export type Visual = "waiting" | "working" | "done" | "returned" | "failed" | "skipped" | "you";

export const VISUAL: Record<Visual, { label: string; glyph: Glyph; desc: string }> = {
  waiting: { label: "Aguardando", glyph: "dot", desc: "ainda não recebeu o trabalho" },
  working: { label: "Trabalhando", glyph: "dot", desc: "a sessão do CLI está em andamento" },
  done: { label: "Concluída", glyph: "check", desc: "entregou a saída e o trabalho seguiu" },
  returned: { label: "Devolvida", glyph: "back", desc: "devolveu o trabalho para ajustes" },
  failed: { label: "Falhou", glyph: "x", desc: "parou por erro ou limite, sem entregar" },
  skipped: { label: "Pulada", glyph: "skip", desc: "não rodou nesta execução" },
  you: { label: "Esperando você", glyph: "you", desc: "precisa da sua decisão para seguir" },
};

export type Glyph = "dot" | "check" | "x" | "back" | "skip" | "you" | "doc" | "edit" | "run" | "search" | "link" | "write" | "swap" | "branch" | "folder" | "alert" | "arrow" | "chev" | "play";

export const GLYPHS: Record<Glyph, string> = {
  dot: '<circle cx="12" cy="12" r="4" fill="currentColor" stroke="none"/>',
  check: '<path d="M5 12.5l4.5 4.5L19 7.5"/>',
  x: '<path d="M6 6l12 12M18 6L6 18"/>',
  back: '<path d="M9 14L4 9l5-5"/><path d="M4 9h10.5a5.5 5.5 0 0 1 0 11H11"/>',
  skip: '<path d="M5 6l6 6-6 6"/><path d="M13 6l6 6-6 6"/>',
  you: '<circle cx="12" cy="8" r="4"/><path d="M4.5 20.5a7.5 7.5 0 0 1 15 0"/>',
  doc: '<path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z"/><path d="M14 3v5h5"/>',
  edit: '<path d="M12 20h9"/><path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4z"/>',
  run: '<path d="M4 17l6-5-6-5"/><path d="M12 19h8"/>',
  search: '<circle cx="11" cy="11" r="7"/><path d="M21 21l-4.3-4.3"/>',
  link: '<path d="M10 13a5 5 0 0 0 7.5.5l3-3a5 5 0 0 0-7-7l-1.7 1.7"/><path d="M14 11a5 5 0 0 0-7.5-.5l-3 3a5 5 0 0 0 7 7l1.7-1.7"/>',
  write: '<path d="M21 15a2 2 0 0 1-2 2H8l-5 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/>',
  swap: '<path d="M7 4L3 8l4 4"/><path d="M3 8h14"/><path d="M17 20l4-4-4-4"/><path d="M21 16H7"/>',
  branch: '<path d="M6 3v12"/><circle cx="18" cy="6" r="3"/><circle cx="6" cy="18" r="3"/><path d="M18 9a9 9 0 0 1-9 9"/>',
  folder: '<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>',
  alert: '<path d="M12 4L2.5 20h19z"/><path d="M12 10v4"/><path d="M12 17.3v.2"/>',
  arrow: '<path d="M5 12h14"/><path d="M13 6l6 6-6 6"/>',
  chev: '<path d="M9 6l6 6-6 6"/>',
  play: '<path d="M7 5l12 7-12 7z"/>',
};

/** SVG de um ícone, para usar com {@html}. */
export function icon(name: Glyph, size = 14, stroke = 2): string {
  return `<svg aria-hidden="true" width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round">${GLYPHS[name]}</svg>`;
}

/** Situação da execução: tom da pílula e rótulo. */
export const RUN_STATUS: Record<PipelineStatus, { tone: "acc" | "warn" | "ok" | "bad" | ""; label: string }> = {
  running: { tone: "acc", label: "Em execução" },
  awaiting_approval: { tone: "warn", label: "Aguardando aprovação" },
  needs_info: { tone: "bad", label: "Precisa de informação" },
  done: { tone: "ok", label: "Concluída" },
  failed: { tone: "bad", label: "Falhou" },
  cancelled: { tone: "", label: "Cancelada" },
};

export const isActive = (status: PipelineStatus) => status === "running" || status === "awaiting_approval";

/** Visual e rótulo de uma etapa registrada. `asking`: o bot pede autorização ao usuário. */
export function stageVisual(stage: StageRun, asking: boolean): { visual: Visual; label: string } {
  const role = stage.role;
  switch (stage.status) {
    case "running":
      if (role === "approval") return { visual: "you", label: "Esperando você" };
      return asking ? { visual: "you", label: "Pede autorização" } : { visual: "working", label: "Trabalhando" };
    case "done":
      if (role === "feedback") return { visual: "you", label: "Ajuste pedido" };
      if (role === "approval") return { visual: "done", label: "Aprovado" };
      if (role === "reviewer" || role === "tester") return { visual: "done", label: "Aprovou" };
      return { visual: "done", label: "Concluída" };
    case "returned":
      if (role === "approval") return { visual: "returned", label: "Ajustes pedidos" };
      if (role === "triage") return { visual: "returned", label: "Achou vaga" };
      return { visual: "returned", label: "Devolveu" };
    case "failed":
      return { visual: "failed", label: "Falhou" };
    case "cancelled":
      return { visual: "skipped", label: "Cancelada" };
  }
}

/** Etapas de cada papel, na ordem em que rodaram. */
export function passesByRole(detail: PipelineDetail): Record<PipelineRole, StageRun[]> {
  const out = Object.fromEntries([...STATIONS, FEEDBACK_STEP].map((s) => [s.role, [] as StageRun[]])) as Record<PipelineRole, StageRun[]>;
  for (const stage of detail.stages) out[stage.role].push(stage);
  return out;
}

/** Nome da passagem para abas e registro: "volta 1/3", "plano v2"… */
export function passLabel(stage: StageRun, index: number, maxRounds: number): string | null {
  if (stage.role === "implementer" && stage.round > 0) return `volta ${stage.round}/${maxRounds}`;
  if ((stage.role === "planner" || stage.role === "approval") && index > 0) return `plano v${index + 1}`;
  if (stage.role === "reviewer" && index > 0) return `${index + 1}ª revisão`;
  if (index > 0) return `${index + 1}ª passagem`;
  return null;
}

/** O que a etapa recebeu, em poucas palavras. */
export function inputSummary(stage: StageRun, detail: PipelineDetail): string {
  switch (stage.role) {
    case "triage": return detail.jira_key ? `Task ${detail.jira_key}` : "Pedido escrito";
    case "planner":
      if (stage.input.includes("<ajustes>")) return "Plano + seus ajustes";
      return stage.input.includes("<triagem>") ? "Task + triagem" : "Task (sem triagem)";
    case "approval": return "Plano";
    case "implementer":
      if (stage.input.includes("<problemas>")) return "Problemas apontados";
      return stage.input.includes("<ajuste_do_usuario>") ? "Seu ajuste" : "Plano aprovado";
    case "reviewer": return stage.input.includes("<diff>") ? "Só o diff" : "Resumo do Implementador";
    case "tester": return "Alterações aprovadas";
    case "reporter": return "Plano, implementação, revisão e testes";
    case "feedback": return "Seu teste";
  }
}

const VERDICT_LINE = /^\W*VEREDITO\W.*$/im;

/** A entrega da etapa em poucas palavras: o veredito ou a primeira linha com conteúdo. */
export function outputSummary(stage: StageRun): string {
  if (stage.status === "running") return stage.role === "approval" ? "esperando você" : "trabalhando…";
  if (stage.status === "cancelled") return "Cancelada por você";
  if (stage.role === "approval") return stage.status === "done" ? "Aprovado por você" : "Ajustes pedidos";
  if (stage.role === "feedback") return firstLine(stage.output ?? "") || "Ajuste pedido";
  const text = stage.output ?? "";
  if (stage.status === "failed") return firstLine(text) || "Falhou";
  const verdict = text.match(VERDICT_LINE)?.[0].replace(/[*`_]/g, "").trim();
  if (verdict) return verdict;
  return firstLine(text) || "Concluída";
}

function firstLine(text: string): string {
  const line = text
    .split("\n")
    .map((l) => l.replace(/^[#>*\-\s`]+/, "").trim())
    .find((l) => l.length > 0);
  if (!line) return "";
  return line.length > 90 ? `${line.slice(0, 89)}…` : line;
}

/** Linha do log de uma etapa. */
export interface LogLine {
  at: number;
  glyph: Glyph;
  verb: string;
  target: string;
  tone: "" | "warn" | "error" | "swap";
}

const VERB_GLYPH: Record<string, Glyph> = {
  Ler: "doc", Editar: "edit", "Criar arquivo": "edit", Executar: "run", Buscar: "search",
  "Listar arquivos": "search", "Buscar na web": "search", "Ler página": "link",
};

/** O que o bot fez, a partir das mensagens gravadas na conversa da etapa. */
export function stageLog(messages: Message[]): LogLine[] {
  const lines: LogLine[] = [];
  // Só o turno da etapa: o pedido é a primeira mensagem do usuário.
  for (const m of messages) {
    if (m.role === "tool") {
      const use = parseToolUse(m.content);
      if (!use) continue;
      const { label, target } = summarize(use.name, use.input);
      const glyph = use.name.startsWith("mcp__") ? "link" : (VERB_GLYPH[label] ?? "run");
      lines.push({ at: m.created_at, glyph: use.is_error ? "x" : glyph, verb: label, target, tone: use.is_error ? "error" : "" });
    } else if (m.role === "notice") {
      const swap = /conta/i.test(m.content);
      lines.push({ at: m.created_at, glyph: swap ? "swap" : "alert", verb: swap ? "Conta" : "Aviso", target: m.content, tone: swap ? "swap" : "warn" });
    } else if (m.role === "assistant" && m.content.trim()) {
      lines.push({ at: m.created_at, glyph: "write", verb: "Escreveu", target: firstLine(m.content), tone: "" });
    }
  }
  return lines;
}

/** Conta que respondeu por último na conversa da etapa. */
export function stageAccountId(messages: Message[]): string | null {
  for (let i = messages.length - 1; i >= 0; i--) {
    if (messages[i].role === "assistant" && messages[i].account_id) return messages[i].account_id;
  }
  return null;
}

/** A conversa da etapa trocou de conta no meio do caminho. */
export function switchedAccount(messages: Message[]): boolean {
  const ids = new Set(messages.filter((m) => m.role === "assistant" && m.account_id).map((m) => m.account_id));
  return ids.size > 1;
}

/** "mm:ss" ou "h:mm:ss". */
export function clock(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  const h = Math.floor(s / 3600);
  const mm = String(Math.floor((s % 3600) / 60)).padStart(2, "0");
  const ss = String(s % 60).padStart(2, "0");
  return h > 0 ? `${h}:${mm}:${ss}` : `${mm}:${ss}`;
}

/** Duração curta: "4:07". */
export function duration(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

/** Nome do serviço e do CLI que o atende. */
export const SERVICE: Record<Provider, { label: string; cli: string }> = {
  claude: { label: "Claude", cli: "Claude Code" },
  chatgpt: { label: "ChatGPT", cli: "Codex" },
  gemini: { label: "Gemini", cli: "Antigravity" },
};

const CLAUDE_TIER: Record<Tier, string> = { fast: "haiku", balanced: "sonnet", strong: "opus" };

/** "Opus 5.5 · Claude", "gpt-5.5 · ChatGPT" ou "padrão do Codex · ChatGPT". */
function modelText(provider: Provider, model: string | null | undefined): string {
  const name = provider === "claude" ? modelLabel(model || "opus") : (model || `padrão do ${SERVICE[provider].cli}`);
  return `${name} · ${SERVICE[provider].label}`;
}

/** Modelo com que uma etapa rodou (ou roda) no serviço da conta que a atende. */
export function stageModelText(detail: PipelineDetail, stage: StageRun, provider: Provider): string | null {
  const models = detail.stage_models[stage.id];
  if (!models) return null;
  return modelText(provider, provider === "claude" ? models.claude : models.others[provider]);
}

/** Modelo que a etapa vai usar no serviço: o do agente (só no Claude) ou o do nível do papel. */
export function plannedModelText(role: PipelineRole, provider: Provider, models: TierModels | null, agentModel: string | null): string | null {
  const tier = STATION[role].tier;
  if (!tier) return null;
  if (provider === "claude") return modelText("claude", agentModel || models?.claude[tier] || CLAUDE_TIER[tier]);
  return modelText(provider, models?.[provider][tier]);
}
