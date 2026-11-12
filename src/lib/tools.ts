// Como apresentar ao usuário o uso de uma ferramenta pelo agente.

export interface ToolUse {
  name: string;
  input: Record<string, unknown>;
  output: string | null;
  is_error: boolean;
}

export interface ToolSummary {
  /** Verbo da ação, ex.: "Editar". */
  label: string;
  /** Alvo da ação, ex.: caminho do arquivo ou comando. */
  target: string;
}

const LABELS: Record<string, string> = {
  Read: "Ler",
  Edit: "Editar",
  MultiEdit: "Editar",
  Write: "Criar arquivo",
  NotebookEdit: "Editar notebook",
  Bash: "Executar",
  Grep: "Buscar",
  Glob: "Listar arquivos",
  WebSearch: "Buscar na web",
  WebFetch: "Ler página",
  Skill: "Skill",
  Task: "Subagente",
  Agent: "Subagente",
  TodoWrite: "Atualizar tarefas",
};

/** Campo dos argumentos que melhor identifica o alvo, por ordem de preferência. */
const TARGET_FIELDS = ["file_path", "notebook_path", "command", "pattern", "query", "url", "skill", "description", "path"];

export function parseToolUse(content: string): ToolUse | null {
  try {
    const value = JSON.parse(content);
    if (typeof value?.name !== "string") return null;
    return { name: value.name, input: value.input ?? {}, output: value.output ?? null, is_error: value.is_error === true };
  } catch {
    return null;
  }
}

export function summarize(name: string, input: Record<string, unknown>): ToolSummary {
  const field = TARGET_FIELDS.find((f) => typeof input[f] === "string" && input[f]);
  const target = field ? String(input[field]) : "";
  const connector = name.match(/^mcp__(.+?)__(.+)$/);
  if (connector) return { label: `Conector ${connector[1]}`, target: connector[2] };
  return { label: LABELS[name] ?? name, target };
}

/** Par antes/depois de uma edição, quando a ferramenta altera um trecho de arquivo. */
export function editDiff(input: Record<string, unknown>): { before: string; after: string } | null {
  if (typeof input.old_string === "string" && typeof input.new_string === "string") {
    return { before: input.old_string, after: input.new_string };
  }
  if (typeof input.content === "string") return { before: "", after: input.content };
  return null;
}

export function parseInput(json: string): Record<string, unknown> {
  try {
    const value = JSON.parse(json);
    return value && typeof value === "object" ? value : {};
  } catch {
    return {};
  }
}
