// Tasks do Jira lidas pelo conector do Atlassian, para escolher a da esteira. Cada consulta passa
// por um turno do Claude Code e leva alguns segundos, então tudo fica guardado entre aberturas do
// app, por conta (cada conta pode ver outro Jira): os quadros com as colunas, a coluna escolhida e
// a última lista de cada coluna. A lista traz só as tasks dos status da coluna escolhida.

import { api, errorMessage } from "../api";
import type { JiraBoard, JiraColumn, JiraIssue } from "../types";

const ISSUES_PREFIX = "lume.jira_issues:";
const BOARDS_PREFIX = "lume.jira_boards:";
const PICK_PREFIX = "lume.jira_pick:";
/** Depois disso, abrir o seletor atualiza a lista da coluna sozinho. */
const STALE_MS = 5 * 60 * 1000;
/** Colunas mudam pouco: os quadros são relidos uma vez por dia (ou pelo botão). */
const BOARDS_STALE_MS = 24 * 60 * 60 * 1000;

interface SavedIssues {
  site: string | null;
  issues: JiraIssue[];
  fetchedAt: number;
}

interface SavedBoards {
  site: string | null;
  boards: JiraBoard[];
  fetchedAt: number;
}

interface Pick {
  boardId: number | null;
  column: string | null;
}

/** Identidade estável da coluna: os ids dos status dela. */
export function columnKey(column: JiraColumn): string {
  return column.statuses.join(",");
}

function read<T>(key: string): T | null {
  try {
    const raw = localStorage.getItem(key);
    return raw ? (JSON.parse(raw) as T) : null;
  } catch {
    return null;
  }
}

function write(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // Sem armazenamento, só não sobrevive ao app fechar.
  }
}

/** Coluna inicial: a primeira que não é o backlog (é por onde o trabalho começa). */
function defaultColumn(board: JiraBoard | undefined): string | null {
  const column = board?.columns.find((c) => c.name !== "Backlog") ?? board?.columns[0];
  return column ? columnKey(column) : null;
}

class JiraState {
  site = $state<string | null>(null);
  /** Tasks da coluna escolhida. */
  issues = $state<JiraIssue[]>([]);
  /** Quando a lista da coluna chegou (ms); 0 = nunca nesta máquina. */
  fetchedAt = $state(0);
  loading = $state(false);
  error = $state<string | null>(null);

  boards = $state<JiraBoard[]>([]);
  boardsFetchedAt = $state(0);
  boardsLoading = $state(false);
  boardsError = $state<string | null>(null);
  boardId = $state<number | null>(null);
  /** Chave (`columnKey`) da coluna escolhida. */
  column = $state<string | null>(null);

  /** Busca em todo o Jira (inclui concluídas), separada da lista da coluna. */
  searchQuery = $state<string | null>(null);
  searchResults = $state<JiraIssue[]>([]);
  searching = $state(false);
  searchError = $state<string | null>(null);

  private inflight: Promise<void> | null = null;
  /** Conta e coluna da consulta em andamento: a de outra não serve para esta. */
  private inflightFor = "";
  private boardsInflight: Promise<void> | null = null;
  private searchSeq = 0;
  /** Conta a que a tela pertence; `undefined` antes da primeira escolha. */
  private forAccount: string | null | undefined;

  get board(): JiraBoard | undefined {
    return this.boards.find((b) => b.id === this.boardId);
  }

  get currentColumn(): JiraColumn | undefined {
    return this.board?.columns.find((c) => columnKey(c) === this.column);
  }

  private issuesKey(accountId: string, column: string | null): string {
    return `${ISSUES_PREFIX}${accountId}${column ? `:${column}` : ""}`;
  }

  /** Mostra o que a conta tem guardado; outra conta descarta o que estava na tela. */
  useAccount(accountId: string | null) {
    if (this.forAccount === accountId) return;
    this.forAccount = accountId;
    const boards = accountId ? read<SavedBoards>(BOARDS_PREFIX + accountId) : null;
    this.boards = Array.isArray(boards?.boards) ? boards.boards : [];
    this.boardsFetchedAt = boards?.fetchedAt ?? 0;
    this.boardsError = null;
    const pick = accountId ? read<Pick>(PICK_PREFIX + accountId) : null;
    this.applyPick(pick?.boardId ?? null, pick?.column ?? null);
    this.site = boards?.site ?? null;
    this.loadSavedIssues();
    this.clearSearch();
  }

  /** Escolha válida para os quadros que existem; o que sumiu volta ao padrão. */
  private applyPick(boardId: number | null, column: string | null) {
    // Sem escolha, o quadro com mais colunas: costuma ser o do fluxo de desenvolvimento.
    const fullest = [...this.boards].sort((a, b) => b.columns.length - a.columns.length)[0];
    const board = this.boards.find((b) => b.id === boardId) ?? fullest;
    this.boardId = board?.id ?? null;
    this.column = board?.columns.some((c) => columnKey(c) === column) ? column : defaultColumn(board);
  }

  private loadSavedIssues() {
    // A lista sem coluna (de antes dos quadros) só vale quando o Jira não tem quadro nenhum: enquanto
    // as colunas não chegam, a tela espera por elas em vez de mostrar tasks de todos os status.
    const withoutColumn = !this.column && !(this.boardsFetchedAt && !this.boards.length);
    const saved = this.forAccount && !withoutColumn ? read<SavedIssues>(this.issuesKey(this.forAccount, this.column)) : null;
    const valid = saved && Array.isArray(saved.issues) ? saved : null;
    if (valid?.site) this.site = valid.site;
    this.issues = valid?.issues ?? [];
    this.fetchedAt = valid?.fetchedAt ?? 0;
    this.error = null;
  }

  /** Ao abrir a tela da esteira: mostra o guardado e só consulta o Jira se a conta nunca consultou. */
  async prefetch(accountId: string | null): Promise<void> {
    this.useAccount(accountId);
    if (!this.boardsFetchedAt) await this.loadBoards();
    if (!this.fetchedAt) await this.refresh();
  }

  /** Ao abrir o seletor: relê os quadros se estão velhos e a coluna se está velha ou falhou. */
  async ensure(accountId: string | null): Promise<void> {
    this.useAccount(accountId);
    if (!this.boardsFetchedAt || Date.now() - this.boardsFetchedAt > BOARDS_STALE_MS || this.boardsError) await this.loadBoards();
    if (this.fetchedAt && Date.now() - this.fetchedAt < STALE_MS && !this.error) return;
    await this.refresh();
  }

  /** Lê os quadros e as colunas; mantém a escolha se ela ainda existe. */
  loadBoards(): Promise<void> {
    if (this.boardsInflight) return this.boardsInflight;
    this.boardsLoading = true;
    this.boardsError = null;
    const expected = this.forAccount;
    const request: Promise<void> = api
      .listJiraBoards()
      .then((result) => {
        const fetchedAt = Date.now();
        const owner = result.account_id ?? expected;
        if (owner) write(BOARDS_PREFIX + owner, { site: result.site, boards: result.boards, fetchedAt });
        if (this.forAccount !== expected) return;
        const before = this.column;
        this.boards = result.boards;
        this.boardsFetchedAt = fetchedAt;
        if (result.site) this.site = result.site;
        this.applyPick(this.boardId, this.column);
        if (this.column !== before) {
          this.savePick();
          this.loadSavedIssues();
        }
      })
      .catch((e) => {
        if (this.forAccount === expected) this.boardsError = errorMessage(e);
      })
      .finally(() => {
        this.boardsLoading = false;
        this.boardsInflight = null;
      });
    this.boardsInflight = request;
    return request;
  }

  private savePick() {
    if (this.forAccount) write(PICK_PREFIX + this.forAccount, { boardId: this.boardId, column: this.column } satisfies Pick);
  }

  /** Troca de quadro: vai para a coluna padrão dele. */
  selectBoard(boardId: number) {
    if (boardId === this.boardId) return;
    this.boardId = boardId;
    this.selectColumn(defaultColumn(this.board));
  }

  /** Troca de coluna: mostra a lista guardada dela e consulta o Jira se está velha. */
  selectColumn(column: string | null) {
    this.clearSearch();
    if (column === this.column && this.fetchedAt) return;
    this.column = column;
    this.savePick();
    this.loadSavedIssues();
    if (!this.fetchedAt || Date.now() - this.fetchedAt >= STALE_MS) void this.refresh();
  }

  /** Consulta a coluna escolhida de novo; chamadas simultâneas para a mesma coluna compartilham a consulta. */
  refresh(): Promise<void> {
    const expectedAccount = this.forAccount;
    const expectedColumn = this.column;
    // Sem coluna enquanto os quadros não vieram (ou falharam), não há o que consultar ainda.
    if (!expectedColumn && !(this.boardsFetchedAt && !this.boards.length)) return Promise.resolve();
    const target = `${expectedAccount ?? ""}|${expectedColumn ?? ""}`;
    if (this.inflight && this.inflightFor === target) return this.inflight;
    this.loading = true;
    this.error = null;
    this.inflightFor = target;
    const statuses = this.currentColumn?.statuses ?? null;
    const request: Promise<void> = api
      .listJiraIssues(null, statuses)
      .then((list) => {
        const fetchedAt = Date.now();
        // Guardada sob a conta que respondeu (pode ser a seguinte, se a ativa estava no limite).
        const owner = list.account_id ?? expectedAccount;
        if (owner) write(this.issuesKey(owner, expectedColumn), { site: list.site, issues: list.issues, fetchedAt } satisfies SavedIssues);
        // A conta ou a coluna mudou enquanto a consulta rodava: a resposta não é da que está na tela.
        if (this.forAccount !== expectedAccount || this.column !== expectedColumn) return;
        if (list.site) this.site = list.site;
        this.issues = list.issues;
        this.fetchedAt = fetchedAt;
      })
      .catch((e) => {
        if (this.forAccount === expectedAccount && this.column === expectedColumn) this.error = errorMessage(e);
      })
      .finally(() => {
        // Uma consulta mais nova (de outra conta ou coluna) segue valendo.
        if (this.inflight !== request) return;
        this.loading = false;
        this.inflight = null;
      });
    this.inflight = request;
    return request;
  }

  /** Procura por texto ou chave em todo o Jira; a resposta de uma busca antiga é descartada. */
  async search(query: string): Promise<void> {
    const q = query.trim();
    if (!q) return this.clearSearch();
    const seq = ++this.searchSeq;
    this.searchQuery = q;
    this.searching = true;
    this.searchError = null;
    this.searchResults = [];
    try {
      const list = await api.listJiraIssues(q);
      if (seq !== this.searchSeq) return;
      if (list.site) this.site = list.site;
      this.searchResults = list.issues;
    } catch (e) {
      if (seq === this.searchSeq) this.searchError = errorMessage(e);
    } finally {
      if (seq === this.searchSeq) this.searching = false;
    }
  }

  clearSearch() {
    this.searchSeq++;
    this.searchQuery = null;
    this.searchResults = [];
    this.searching = false;
    this.searchError = null;
  }
}

export const jira = new JiraState();
