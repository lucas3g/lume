// Logins do Claude em andamento. Às vezes o login termina numa página com um código
// de autenticação para colar no app; o estado fica fora das telas para o campo do
// código continuar disponível quando o usuário sai e volta enquanto o login espera.

/** Onde o login foi aberto; o código colado vai para o login daquele lugar. */
export type CodeScope = "account" | "connector";

class LoginsState {
  /** Login de conta do Claude esperando o navegador (ou o código). */
  account = $state(false);
  /** Conector com login esperando o navegador (ou o código). */
  connector = $state<string | null>(null);

  /** Executa o login de uma conta, marcando-o como em andamento. Um segundo login de conta
   * enquanto outro espera não começa: o código colado iria para o primeiro. */
  async accountLogin(claude: boolean, action: () => Promise<void>): Promise<void> {
    if (this.account) return;
    this.account = claude;
    try {
      await action();
    } finally {
      this.account = false;
    }
  }

  /** Executa o login de um conector, marcando-o como em andamento. */
  async connectorLogin(name: string, action: () => Promise<void>): Promise<void> {
    if (this.connector) return;
    this.connector = name;
    try {
      await action();
    } finally {
      this.connector = null;
    }
  }
}

export const logins = new LoginsState();
