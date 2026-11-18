// Como apresentar a esteira de bots: estações, estados, rótulos e o log de cada etapa.

import { parseToolUse, summarize } from "./tools";
import { modelLabel } from "./types";
import type { Message, PipelineDetail, Provider, Tier, TierModels, PipelineRole, PipelineStatus, Question, ReviewFinding, StageRun, TokenUsage } from "./types";

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
  { role: "code_review", name: "Code review", model: "Modelo forte", tier: "strong", perm: "só leitura",
    does: "Sempre roda a skill code-review do CLI (sem projeto versionado, nos arquivos alterados) e lista todos os achados dela. Você escolhe o que corrigir e decide os que têm mais de um caminho.",
    receives: "As alterações aprovadas e testadas.", delivers: "Os achados, com a gravidade e, quando há, os caminhos possíveis." },
  { role: "reporter", name: "Relator", model: "Modelo rápido", tier: "fast", perm: "Jira",
    does: "Escreve o que foi feito no item Solução da descrição da task (sem comentar), move para Teste e sugere a mensagem de commit. Não faz commit nem push.",
    receives: "Plano, resumo da implementação, revisão e testes.", delivers: "O comentário no Jira, a mudança de status e a sugestão de commit." },
];

export const TIERS: { tier: Tier; label: string }[] = [
  { tier: "fast", label: "Rápido" },
  { tier: "balanced", label: "Equilibrado" },
  { tier: "strong", label: "Forte" },
];

/** Etapas que o projeto pode desligar; as demais garantem plano aprovado e código revisado. */
export const OPTIONAL_ROLES: PipelineRole[] = ["triage", "tester", "code_review", "reporter"];

/** Ajuste que você pede ao reabrir uma execução concluída; aparece no registro, não na esteira. */
export const FEEDBACK_STEP: Station = {
  role: "feedback", name: "Seu ajuste", model: null, tier: null, perm: "você pediu", gate: true,
  does: "Depois de testar o que foi entregue, você descreve o que não funcionou e a esteira volta ao Implementador.",
  receives: "A execução concluída.", delivers: "O que precisa mudar.",
};

/** Designer: desenha no Claude Design antes do plano; só roda quando a execução pede. Aparece na estação do Planejador. */
export const DESIGNER_STEP: Station = {
  role: "designer", name: "Designer", model: "Modelo forte", tier: "strong", perm: "Claude Design",
  does: "Estuda as telas parecidas no código e desenha a solução no Claude Design, publicando o design como artifact. Não altera o projeto.",
  receives: "A task e as notas da Triagem; nas voltas, os seus ajustes.", delivers: "O link do design e uma especificação curta.",
};

/** Sua aprovação do design; aparece no registro e, na esteira, na sua mesa (estação da aprovação). */
export const DESIGN_APPROVAL_STEP: Station = {
  role: "design_approval", name: "Aprovação do design", model: null, tier: null, perm: "você decide", gate: true,
  does: "Você abre o design no Claude Design e aprova ou pede ajustes. O plano só começa depois, seguindo o design aprovado.",
  receives: "O design do Designer.", delivers: "A sua decisão: aprovar o design ou pedir ajustes.",
};

/** Sua escolha sobre o code review; aparece no registro e, na esteira, na estação do Code review. */
export const CHOICE_STEP: Station = {
  role: "review_choice", name: "Sua escolha", model: null, tier: null, perm: "você decide", gate: true,
  does: "Você vê os achados do code review, marca os que quer corrigir e escolhe o caminho dos que têm mais de um. Sem nada marcado, a esteira segue para o Relator.",
  receives: "Os achados do code review.", delivers: "O que corrigir e como.",
};

/** Retomada de uma execução que parou no limite de voltas; aparece no registro, não na esteira. */
export const RESUME_STEP: Station = {
  role: "resume", name: "Sua retomada", model: null, tier: null, perm: "você retomou", gate: true,
  does: "O trabalho foi devolvido o máximo de vezes e a esteira parou. Ao retomar, a contagem de voltas recomeça e o Implementador corrige o que foi apontado por último.",
  receives: "A execução parada no limite de voltas.", delivers: "Voltas zeradas.",
};

/** Etapas fora da lista de estações: aparecem no registro e na estação indicada por `stationRole`. */
const EXTRA_STEPS = [DESIGNER_STEP, DESIGN_APPROVAL_STEP, FEEDBACK_STEP, CHOICE_STEP, RESUME_STEP];

/** Marcas suas que não são mesa nem estação: o ajuste ao reabrir e a retomada no limite de voltas. */
export const isMarker = (role: PipelineRole) => role === "feedback" || role === "resume";

export const STATION = Object.fromEntries([...STATIONS, ...EXTRA_STEPS].map((s) => [s.role, s])) as Record<PipelineRole, Station>;

/** Começo do pedido de correção que a sua escolha manda ao Implementador (ver `fix_request` no Rust). */
const FIX_MARK = "O code review apontou";

/** Etapas que esperam você, não um bot. */
export const isUserStep = (role: PipelineRole) => STATION[role].gate === true;

/** Estação em que a etapa aparece na esteira: a sua escolha fica na do Code review; o Designer, na do Planejador;
 *  a aprovação do design, na sua mesa; o ajuste, em nenhuma. */
export const stationRole = (role: PipelineRole): PipelineRole | null =>
  role === "review_choice" ? "code_review"
  : role === "designer" ? "planner"
  : role === "design_approval" ? "approval"
  : isMarker(role) ? null
  : role;

const DESIGN_LINK = /https:\/\/claude\.ai\/[\w/.:-]*artifact\/[\w-]{6,}/g;

/** Link do design publicado pelo Designer (o da linha `DESIGN:` ou o último link de artifact). */
export function designUrl(text: string): string | null {
  const tagged = text.split("\n").reverse().find((l) => /^\W*DESIGN:/i.test(l.trim()));
  const pick = (s: string) => s.match(DESIGN_LINK)?.at(-1)?.replace(/[./]+$/, "") ?? null;
  return (tagged && pick(tagged)) || pick(text);
}

/** Especificação do design sem a linha do link. */
export function designSpec(text: string): string {
  return text.split("\n").filter((l) => !/^\W*DESIGN:/i.test(l.trim())).join("\n").trim();
}

const JSON_FENCE = "```json";

/** Conteúdo do último bloco ```json do texto (onde o code review lista os achados); `null` sem ele. */
function lastJsonBlock(text: string): string | null {
  const start = text.lastIndexOf(JSON_FENCE);
  return start < 0 ? null : text.slice(start + JSON_FENCE.length).split("```")[0];
}

const LIST_ITEM = /^(\s*)(?:[-*+]|\d+[.)])\s+/;

/** Perguntas com opções do último bloco ```json da Triagem; `null` sem bloco válido. */
function triageJsonQuestions(text: string): Question[] | null {
  const block = lastJsonBlock(text);
  if (block === null) return null;
  let items: unknown;
  try {
    items = JSON.parse(block.trim());
  } catch {
    return null;
  }
  if (!Array.isArray(items)) return null;
  const str = (v: unknown) => (typeof v === "string" ? v.trim() : "");
  const out: Question[] = [];
  for (const item of items) {
    if (!item || typeof item !== "object") continue;
    const q = item as Record<string, unknown>;
    const question = str(q.question);
    if (!question || out.some((o) => o.question === question)) continue;
    const seen = new Set<string>();
    const options = (Array.isArray(q.options) ? q.options : [])
      .map((o: unknown) => {
        if (typeof o === "string") return { label: o.trim() };
        const r = (o ?? {}) as Record<string, unknown>;
        return { label: str(r.label), description: str(r.description) || undefined };
      })
      .filter((o) => {
        if (!o.label || seen.has(o.label)) return false;
        seen.add(o.label);
        return true;
      });
    out.push({ question, header: str(q.header) || undefined, multiSelect: q.multiSelect === true, options });
  }
  return out.length ? out : null;
}

/**
 * Perguntas que a Triagem deixou para o usuário quando achou a task vaga: as do bloco ```json, com opções;
 * sem ele (respostas antigas), os itens do bloco `PERGUNTAS:` ou as linhas que terminam em interrogação, só com
 * o campo livre.
 */
export function triageQuestions(text: string): Question[] {
  const structured = triageJsonQuestions(text);
  if (structured) return structured;
  // Sem repetição: a pergunta é a chave do formulário.
  return [...new Set(triageListQuestions(text))].map((question) => ({ question, options: [] }));
}

/** Análise da Triagem sem o bloco ```json das perguntas (que a tela mostra como formulário). */
export function triageSummary(text: string): string {
  if (!triageJsonQuestions(text)) return text;
  const start = text.lastIndexOf(JSON_FENCE);
  const end = text.indexOf("```", start + JSON_FENCE.length);
  const before = text.slice(0, start).replace(/(^|\n)\W*perguntas\W*:?\W*\s*$/i, "\n");
  return `${before.trimEnd()}\n\n${end < 0 ? "" : text.slice(end + 3).trim()}`.trim();
}

function triageListQuestions(text: string): string[] {
  const clean = (line: string) => line.replace(LIST_ITEM, "").replace(/^>\s*/, "").replace(/\*\*/g, "").trim();
  const lines = text.split("\n");
  // O bloco fica logo antes do veredito: vale o último cabeçalho `PERGUNTAS:` do texto.
  let start = -1;
  lines.forEach((l, i) => { if (/^\W*perguntas\W*:?\W*$/i.test(l.trim())) start = i; });
  if (start >= 0) {
    const out: string[] = [];
    let indent = -1;
    let blank = false;
    for (const line of lines.slice(start + 1)) {
      const trimmed = line.trim();
      if (/^\W*veredito/i.test(trimmed) || /^(#|-{3,}$|\*{3,}$|_{3,}$)/.test(trimmed)) break;
      const item = LIST_ITEM.exec(line);
      if (item && (indent < 0 || item[1].length <= indent)) {
        // Item do mesmo nível: uma pergunta nova.
        indent = item[1].length;
        out.push(clean(line));
      } else if (trimmed && out.length) {
        // Subitem ou continuação da pergunta; texto solto depois de uma linha em branco já é outra coisa.
        if (blank && !item && !/^\s/.test(line)) break;
        out[out.length - 1] += ` ${clean(line)}`;
      }
      blank = !trimmed;
    }
    if (out.length) return out;
  }
  return lines.map(clean).filter((l) => l.endsWith("?") && !/^#/.test(l));
}

/** Texto do code review sem o bloco ```json final dos achados (que a tela mostra como lista). */
export function reviewSummary(text: string): string {
  const start = text.lastIndexOf(JSON_FENCE);
  return (start < 0 ? text : text.slice(0, start)).trim();
}

/** Os achados em Markdown, para o registro da sua escolha. */
export function findingsMarkdown(findings: ReviewFinding[]): string {
  return findings
    .map((f) => {
      const place = f.file ? ` · \`${f.file}${f.line ? `:${f.line}` : ""}\`` : "";
      const options = f.options.map((o, i) => `\n   ${i + 1}. ${o}`).join("");
      return `- **${f.id} · ${f.title}**${f.severity ? ` (${f.severity})` : ""}${place}${f.detail ? `\n  ${f.detail}` : ""}${options}`;
    })
    .join("\n");
}

/** Achados gravados na parada da sua escolha (vazio se o code review não trouxe a lista). */
export function choiceFindings(stage: StageRun): ReviewFinding[] {
  try {
    const list = JSON.parse(stage.input);
    return Array.isArray(list) ? (list as ReviewFinding[]) : [];
  } catch {
    return [];
  }
}

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
  awaiting_choice: { tone: "warn", label: "Aguardando sua escolha" },
  needs_info: { tone: "bad", label: "Precisa de informação" },
  done: { tone: "ok", label: "Concluída" },
  failed: { tone: "bad", label: "Falhou" },
  cancelled: { tone: "", label: "Cancelada" },
};

export const isActive = (status: PipelineStatus) => status === "running" || isWaitingYou(status);
/** A execução está parada esperando uma decisão sua (plano ou code review). */
export const isWaitingYou = (status: PipelineStatus) => status === "awaiting_approval" || status === "awaiting_choice";

/** Visual e rótulo de uma etapa registrada. `asking`: o bot pede autorização ao usuário. */
export function stageVisual(stage: StageRun, asking: boolean): { visual: Visual; label: string } {
  const role = stage.role;
  switch (stage.status) {
    case "running":
      if (role === "approval" || role === "design_approval") return { visual: "you", label: "Esperando você" };
      if (role === "review_choice") return { visual: "you", label: "Esperando sua escolha" };
      return asking ? { visual: "you", label: "Pede autorização" } : { visual: "working", label: "Trabalhando" };
    case "done":
      if (role === "feedback") return { visual: "you", label: "Ajuste pedido" };
      if (role === "resume") return { visual: "you", label: "Retomada" };
      if (role === "approval" || role === "design_approval") return { visual: "done", label: "Aprovado" };
      if (role === "review_choice") return { visual: "done", label: "Sem correções" };
      if (role === "reviewer" || role === "tester") return { visual: "done", label: "Aprovou" };
      return { visual: "done", label: "Concluída" };
    case "returned":
      if (role === "approval" || role === "design_approval") return { visual: "returned", label: "Ajustes pedidos" };
      if (role === "review_choice") return { visual: "returned", label: "Correções pedidas" };
      if (role === "triage") return { visual: "returned", label: "Achou vaga" };
      return { visual: "returned", label: "Devolveu" };
    case "failed":
      return { visual: "failed", label: "Falhou" };
    case "cancelled":
      return { visual: "skipped", label: "Cancelada" };
  }
}

/** Passagens que contam numa estação: os bots que aparecem nela (o Designer conta na do Planejador) e,
 *  nas suas mesas, as suas paradas (a aprovação do design conta na da aprovação). */
export function stationPasses(detail: PipelineDetail, station: PipelineRole): StageRun[] {
  const gate = STATION[station].gate === true;
  return detail.stages.filter((s) => stationRole(s.role) === station && (gate || !isUserStep(s.role)));
}

/** Etapas de cada papel, na ordem em que rodaram. */
export function passesByRole(detail: PipelineDetail): Record<PipelineRole, StageRun[]> {
  const out = Object.fromEntries([...STATIONS, ...EXTRA_STEPS].map((s) => [s.role, [] as StageRun[]])) as Record<PipelineRole, StageRun[]>;
  for (const stage of detail.stages) out[stage.role].push(stage);
  return out;
}

/** Nome da passagem para abas e registro: "volta 1/3", "plano v2"… */
export function passLabel(stage: StageRun, index: number, maxRounds: number): string | null {
  if (stage.role === "implementer" && stage.input.includes(FIX_MARK)) return "correções do code review";
  if (stage.role === "implementer" && stage.round > 0) return `volta ${stage.round}/${maxRounds}`;
  if ((stage.role === "planner" || stage.role === "approval") && index > 0) return `plano v${index + 1}`;
  if ((stage.role === "designer" || stage.role === "design_approval") && index > 0) return `design v${index + 1}`;
  if (stage.role === "reviewer" && index > 0) return `${index + 1}ª revisão`;
  if (index > 0) return `${index + 1}ª passagem`;
  return null;
}

/** O que a etapa recebeu, em poucas palavras. */
export function inputSummary(stage: StageRun, detail: PipelineDetail): string {
  switch (stage.role) {
    case "triage": return detail.jira_key ? `Task ${detail.jira_key}` : "Pedido escrito";
    case "designer":
      if (stage.input.includes("<ajustes>")) return "Design + seus ajustes";
      return stage.input.includes("<triagem>") ? "Task + triagem" : "Task (sem triagem)";
    case "design_approval": return "Design";
    case "planner":
      if (stage.input.includes("<ajustes>")) return "Plano + seus ajustes";
      if (stage.input.includes("<design_aprovado>")) return "Task + design aprovado";
      return stage.input.includes("<triagem>") ? "Task + triagem" : "Task (sem triagem)";
    case "approval": return "Plano";
    case "implementer":
      if (stage.input.includes(FIX_MARK)) return "Correções do code review";
      if (stage.input.includes("<problemas>")) return "Problemas apontados";
      return stage.input.includes("<ajuste_do_usuario>") ? "Seu ajuste" : "Plano aprovado";
    case "reviewer": return stage.input.includes("<diff>") ? "Só o diff" : "Resumo do Implementador";
    case "tester": return "Alterações aprovadas";
    case "code_review": return "Alterações aprovadas e testadas";
    case "review_choice": return "Achados do code review";
    case "reporter": return stage.input.includes("<code_review>") ? "Plano, implementação, revisão, testes e code review" : "Plano, implementação, revisão e testes";
    case "feedback": return "Seu teste";
    case "resume": return "Limite de voltas";
  }
}

const VERDICT_LINE = /^\W*VEREDITO\W.*$/im;

/** O que o Relator diz que fez no Jira: a linha `JIRA:` e, se a criação falhou, o motivo de `TASK_CRIADA: nenhuma`. */
export function jiraOutcome(report: string): { failed: boolean; text: string } | null {
  const line = (tag: string) =>
    report.split("\n").map((l) => l.trim().replace(/^[*`_ ]+|[*`_ ]+$/g, "")).findLast((l) => l.toUpperCase().startsWith(tag))?.slice(tag.length).trim();
  const created = line("TASK_CRIADA:");
  if (created && /^nenhuma\b/i.test(created)) return { failed: true, text: created.replace(/^nenhuma\s*/i, "").replace(/^\((.*)\)$/, "$1") || "sem motivo informado" };
  const jira = line("JIRA:");
  if (!jira) return null;
  return /^falhou\b/i.test(jira) ? { failed: true, text: jira.replace(/^falhou\s*/i, "").replace(/^\((.*)\)$/, "$1") || "sem motivo informado" } : { failed: false, text: jira };
}

/** A entrega da etapa em poucas palavras: o veredito ou a primeira linha com conteúdo. */
export function outputSummary(stage: StageRun): string {
  if (stage.status === "running") return isUserStep(stage.role) ? "esperando você" : "trabalhando…";
  if (stage.status === "cancelled") return "Cancelada por você";
  if (stage.role === "approval" || stage.role === "design_approval") return stage.status === "done" ? "Aprovado por você" : "Ajustes pedidos";
  if (stage.role === "designer" && stage.status === "done") return designUrl(stage.output ?? "") ? "Design publicado" : "Concluída";
  if (stage.role === "feedback") return firstLine(stage.output ?? "") || "Ajuste pedido";
  if (stage.role === "resume") return "Voltas zeradas por você";
  if (stage.role === "review_choice") return stage.status === "done" ? "Seguir sem correções" : "Correções escolhidas por você";
  if (stage.role === "code_review" && stage.status === "done") {
    const found = findingsIn(stage.output ?? "");
    if (found !== null) return found === 0 ? "Nenhum achado" : found === 1 ? "1 achado" : `${found} achados`;
  }
  const text = stage.output ?? "";
  if (stage.status === "failed") return firstLine(text) || "Falhou";
  const verdict = text.match(VERDICT_LINE)?.[0].replace(/[*`_]/g, "").trim();
  if (verdict) return verdict;
  return firstLine(text) || "Concluída";
}

/** Itens do último bloco ```json da resposta do code review que viram achados (com título ou
 *  detalhe, como no backend); `null` sem a lista. */
function findingsIn(text: string): number | null {
  const block = lastJsonBlock(text);
  if (block === null) return null;
  try {
    const list = JSON.parse(block);
    return Array.isArray(list) ? list.filter((f) => f && (String(f.title ?? "").trim() || String(f.detail ?? "").trim())).length : null;
  } catch {
    return null;
  }
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

/** Todos os tokens processados: entrada (nova e do cache) e saída. */
export const totalTokens = (u: TokenUsage) => u.input + u.output + u.cache_read + u.cache_write;

/** Quantidade curta: "850", "12 mil", "1,4 mi". Arredonda antes de escolher a unidade: 999.600 é "1 mi". */
export function tokenCount(n: number): string {
  const round = (v: number) => (v < 10 ? Math.round(v * 10) / 10 : Math.round(v));
  const short = (v: number) => v.toLocaleString("pt-BR", { maximumFractionDigits: 1 });
  if (n < 1000) return String(n);
  const thousands = round(n / 1000);
  if (thousands < 1000) return `${short(thousands)} mil`;
  return `${short(round(n / 1_000_000))} mi`;
}

/** Custo equivalente na API: "US$ 0,42". */
export const usd = (v: number) => `US$ ${v.toLocaleString("pt-BR", { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`;

/** Custo para exibir: "US$ 1,24", "US$ 1,24+" quando parte dos tokens veio sem preço; null sem custo informado. */
export const costText = (u: TokenUsage) => (u.cost_usd != null ? `${usd(u.cost_usd)}${u.unpriced ? "+" : ""}` : null);

/** Resumo do gasto: "312 mil tokens · US$ 1,24" (sem custo quando o provedor não informa). */
export function usageText(u: TokenUsage): string {
  const tokens = `${tokenCount(totalTokens(u))} tokens`;
  const cost = costText(u);
  return cost ? `${tokens} · ${cost}` : tokens;
}

/** Detalhe do gasto, para a dica: de onde vieram os tokens. */
export function usageHint(u: TokenUsage): string {
  const parts = [
    `Entrada nova: ${u.input.toLocaleString("pt-BR")}`,
    `Lida do cache: ${u.cache_read.toLocaleString("pt-BR")}`,
    `Gravada no cache: ${u.cache_write.toLocaleString("pt-BR")}`,
    `Saída: ${u.output.toLocaleString("pt-BR")}`,
  ];
  if (u.cost_usd != null) parts.push(`Custo equivalente na API: ${usd(u.cost_usd)} (com assinatura não há cobrança; serve para comparar as etapas)`);
  if (u.cost_usd != null && u.unpriced) parts.push("Custo parcial: parte dos tokens veio sem preço (Codex, ou etapa interrompida antes do fim)");
  return parts.join("\n");
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
