// Tasks do Jira lidas pelo conector do Atlassian, para escolher a da esteira. A consulta passa
// por um turno do Claude Code e leva perto de um minuto: a última lista da conta aparece na hora
// (guardada entre aberturas do app, uma por conta, porque cada conta pode ver outro Jira) e é
// atualizada por trás quando o seletor abre com ela velha.

import { api, errorMessage } from "../api";
import type { JiraIssue } from "../types";

const STORE_PREFIX = "lume.jira_issues:";
/** Depois disso, abrir o seletor atualiza a lista sozinho. */
const STALE_MS = 5 * 60 * 1000;

interface Saved {
  site: string | null;
  issues: JiraIssue[];
  fetchedAt: number;
}

function readSaved(accountId: string): Saved | null {
  try {
    const raw = localStorage.getItem(STORE_PREFIX + accountId);
    const saved = raw ? (JSON.parse(raw) as Saved) : null;
    return saved && Array.isArray(saved.issues) ? saved : null;
  } catch {
    return null;
  }
}

class JiraState {
  site = $state<string | null>(null);
  issues = $state<JiraIssue[]>([]);
  /** Quando a lista chegou (ms); 0 = nunca nesta máquina. */
  fetchedAt = $state(0);
  loading = $state(false);
  error = $state<string | null>(null);

  /** Busca em todo o Jira (inclui concluídas), separada da lista base. */
  searchQuery = $state<string | null>(null);
  searchResults = $state<JiraIssue[]>([]);
  searching = $state(false);
  searchError = $state<string | null>(null);

  private inflight: Promise<void> | null = null;
  /** Conta da consulta em andamento: a de outra conta não serve para esta. */
  private inflightAccount: string | null | undefined;
  private searchSeq = 0;
  /** Conta a que a lista mostrada pertence; `undefined` antes da primeira escolha. */
  private forAccount: string | null | undefined;

  /** Mostra a lista guardada da conta; outra conta descarta a que estava na tela. */
  useAccount(accountId: string | null) {
    if (this.forAccount === accountId) return;
    this.forAccount = accountId;
    const saved = accountId ? readSaved(accountId) : null;
    this.site = saved?.site ?? null;
    this.issues = saved?.issues ?? [];
    this.fetchedAt = saved?.fetchedAt ?? 0;
    this.error = null;
    this.clearSearch();
  }

  /** Ao abrir a tela da esteira: mostra a lista guardada e só consulta o Jira se a conta nunca teve uma. */
  prefetch(accountId: string | null): Promise<void> {
    this.useAccount(accountId);
    return this.fetchedAt ? Promise.resolve() : this.refresh();
  }

  /** Ao abrir o seletor: atualiza a lista se ela nunca veio, está velha ou falhou. */
  ensure(accountId: string | null): Promise<void> {
    this.useAccount(accountId);
    if (this.fetchedAt && Date.now() - this.fetchedAt < STALE_MS && !this.error) return Promise.resolve();
    return this.refresh();
  }

  /** Consulta o Jira de novo; chamadas simultâneas compartilham a mesma consulta. */
  refresh(): Promise<void> {
    if (this.inflight && this.inflightAccount === this.forAccount) return this.inflight;
    this.loading = true;
    this.error = null;
    const expected = this.forAccount;
    this.inflightAccount = expected;
    const request: Promise<void> = api
      .listJiraIssues(null)
      .then((list) => {
        const fetchedAt = Date.now();
        // Guardada sob a conta que respondeu (pode ser a seguinte, se a ativa estava no limite).
        const owner = list.account_id ?? expected;
        try {
          if (owner) localStorage.setItem(STORE_PREFIX + owner, JSON.stringify({ site: list.site, issues: list.issues, fetchedAt }));
        } catch {
          // Sem armazenamento, a lista só não sobrevive ao app fechar.
        }
        // A conta mudou enquanto a consulta rodava: a resposta não é da que está na tela.
        if (this.forAccount !== expected) return;
        this.site = list.site;
        this.issues = list.issues;
        this.fetchedAt = fetchedAt;
      })
      .catch((e) => {
        if (this.forAccount === expected) this.error = errorMessage(e);
      })
      .finally(() => {
        // Uma consulta mais nova (de outra conta) segue valendo.
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
