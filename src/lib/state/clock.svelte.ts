/**
 * Relógio que só muda quando o dia vira: rótulos como "Hoje" e "Ontem" que dependem de
 * `dayClock.now` são recalculados à meia-noite, sem redesenhar a tela a cada minuto.
 */
export const dayClock = $state({ now: Date.now() });

let timer: ReturnType<typeof setTimeout> | undefined;

function msUntilMidnight(): number {
  const next = new Date();
  next.setHours(24, 0, 1, 0);
  return next.getTime() - Date.now();
}

/** Atualiza só se o dia mudou e agenda a próxima virada. */
function sync() {
  const now = Date.now();
  if (new Date(now).toDateString() !== new Date(dayClock.now).toDateString()) dayClock.now = now;
  clearTimeout(timer);
  timer = setTimeout(sync, msUntilMidnight());
}

if (typeof window !== "undefined") {
  sync();
  // Depois de o computador dormir, o timer pode atrasar: confere ao voltar para o app.
  window.addEventListener("focus", sync);
  document.addEventListener("visibilitychange", () => document.visibilityState === "visible" && sync());
}

/**
 * Relógio por segundo, para tempos que correm na tela (ex.: um comando em execução).
 * Só bate enquanto alguém o usa: cada `useLiveClock` liga e devolve como desligar.
 */
export const liveClock = $state({ now: Date.now() });

let users = 0;
let ticker: ReturnType<typeof setInterval> | undefined;

export function useLiveClock(): () => void {
  users += 1;
  if (users === 1) {
    liveClock.now = Date.now();
    ticker = setInterval(() => (liveClock.now = Date.now()), 1000);
  }
  return () => {
    users -= 1;
    if (users === 0) clearInterval(ticker);
  };
}
