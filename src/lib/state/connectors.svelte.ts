// Conectores da conta ativa, guardados entre visitas à tela: a verificação pelo
// Claude Code leva segundos, então a última lista aparece na hora e é atualizada por trás.

import { api } from "../api";
import type { AccountConnector } from "../types";

class ConnectorsState {
  account = $state<AccountConnector[]>([]);
  /** Verificação em andamento. */
  checking = $state(false);
  /** Já houve uma verificação concluída nesta sessão. */
  loaded = $state(false);
  error = $state<string | null>(null);
  /** Conta a que a lista guardada pertence. */
  private forAccount: string | null | undefined;

  private inflight: Promise<void> | null = null;

  /** Garante a lista da conta ativa: outra conta descarta a guardada; a mesma só atualiza por trás. */
  ensure(accountId: string | null): Promise<void> {
    if (this.forAccount !== undefined && this.forAccount !== accountId) this.invalidate();
    this.forAccount = accountId;
    return this.refresh();
  }

  /** Atualiza a lista; chamadas simultâneas compartilham a mesma verificação. */
  refresh(): Promise<void> {
    if (this.inflight) return this.inflight;
    this.checking = true;
    this.error = null;
    const expected = this.forAccount;
    this.inflight = api
      .listAccountConnectors()
      .then((list) => {
        if (this.forAccount !== expected) return;
        this.account = list;
        this.loaded = true;
      })
      .catch((e) => {
        this.error = e instanceof Error ? e.message : String(e);
      })
      .finally(() => {
        this.checking = false;
        this.inflight = null;
      });
    return this.inflight;
  }

  /** A conta ativa mudou: a lista guardada não vale mais. */
  invalidate() {
    this.account = [];
    this.loaded = false;
  }
}

export const connectors = new ConnectorsState();
