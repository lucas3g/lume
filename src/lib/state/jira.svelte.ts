// Tasks do Jira lidas pelo conector do Atlassian, para escolher a da esteira. A consulta passa
// por um turno do Claude Code e leva alguns segundos: a última lista aparece na hora (guardada
// entre aberturas do app) e é atualizada por trás.

import { api, errorMessage } from "../api";
import type { JiraIssue } from "../types";

const STORE_KEY = "lume.jira_issues";
/** Depois disso, abrir o seletor atualiza a lista sozinho. */
const STALE_MS = 5 * 60 * 1000;

interface Saved {
  site: string | null;
  issues: JiraIssue[];
  fetchedAt: number;
}

function readSaved(): Saved | null {
  try {
    const raw = localStorage.getItem(STORE_KEY);
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
  private searchSeq = 0;

  constructor() {
    const saved = readSaved();
    if (saved) {
      this.site = saved.site;
      this.issues = saved.issues;
      this.fetchedAt = saved.fetchedAt;
    }
  }

  /** Atualiza a lista se ela nunca veio ou está velha. */
  ensure(): Promise<void> {
    if (this.fetchedAt && Date.now() - this.fetchedAt < STALE_MS && !this.error) return Promise.resolve();
    return this.refresh();
  }

  /** Consulta o Jira de novo; chamadas simultâneas compartilham a mesma consulta. */
  refresh(): Promise<void> {
    if (this.inflight) return this.inflight;
    this.loading = true;
    this.error = null;
    this.inflight = api
      .listJiraIssues(null)
      .then((list) => {
        this.site = list.site;
        this.issues = list.issues;
        this.fetchedAt = Date.now();
        try {
          localStorage.setItem(STORE_KEY, JSON.stringify({ site: list.site, issues: list.issues, fetchedAt: this.fetchedAt }));
        } catch {
          // Sem armazenamento, a lista só não sobrevive ao app fechar.
        }
      })
      .catch((e) => {
        this.error = errorMessage(e);
      })
      .finally(() => {
        this.loading = false;
        this.inflight = null;
      });
    return this.inflight;
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
