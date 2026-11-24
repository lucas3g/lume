// Estado do pool de contas e das configurações, compartilhado entre as telas.

import { api } from "../api";
import { nowSeconds } from "../format";
import type { Account, FlagKey, Flags, Switch } from "../types";

export type AccountState = "login" | "disabled" | "limited" | "active" | "available";

export const accounts = $state({
  list: [] as Account[],
  switches: [] as Switch[],
  flags: { auto_switch: true, notify_switch: true, use_history: true, switch_early: true, notify_desktop: true, resume_on_reset: true } as Flags,
});

export function isUsable(a: Account, now = nowSeconds()): boolean {
  return a.enabled && a.auth_ok && (a.limited_until === null || a.limited_until <= now);
}

/** A conta que atenderá a próxima mensagem. */
export function activeAccount(): Account | undefined {
  return accounts.list.find((a) => isUsable(a));
}

export function usableCount(): number {
  return accounts.list.filter((a) => isUsable(a)).length;
}

export function stateOf(a: Account): AccountState {
  if (!a.auth_ok) return "login";
  if (!a.enabled) return "disabled";
  if (!isUsable(a)) return "limited";
  return activeAccount()?.id === a.id ? "active" : "available";
}

export async function loadAccounts(): Promise<void> {
  [accounts.list, accounts.switches] = await Promise.all([api.listAccounts(), api.listSwitches()]);
}

export async function loadFlags(): Promise<void> {
  accounts.flags = await api.getFlags();
}

export async function setFlag(key: FlagKey, value: boolean): Promise<void> {
  accounts.flags[key] = value;
  await api.setFlag(key, value);
}

/**
 * Grava uma nova ordem de prioridade; a lista já aparece na ordem nova enquanto salva.
 * Se a gravação falhar, recarrega para desfazer a ordem otimista. Quem chama recarrega no sucesso.
 */
export async function reorderAccounts(ids: string[]): Promise<void> {
  const byId = new Map(accounts.list.map((a) => [a.id, a]));
  accounts.list = ids.map((id) => byId.get(id)).filter((a): a is Account => !!a);
  try {
    await api.reorderAccounts(ids);
  } catch (e) {
    await loadAccounts().catch(() => {});
    throw e;
  }
}

/** Troca a posição de uma conta com a vizinha na ordem de prioridade. */
export async function moveAccount(id: string, direction: -1 | 1): Promise<void> {
  const ids = accounts.list.map((a) => a.id);
  const from = ids.indexOf(id);
  const to = from + direction;
  if (from < 0 || to < 0 || to >= ids.length) return;
  [ids[from], ids[to]] = [ids[to], ids[from]];
  await reorderAccounts(ids);
}
