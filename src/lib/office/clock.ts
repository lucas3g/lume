/* Relógio do escritório. As animações da sala são CSS em passos (steps), mas enquanto qualquer uma roda o navegador
   produz 60 quadros por segundo, mesmo quando nenhum pixel muda. Aqui elas ficam pausadas (classe .clocked) e o
   relógio lê nos keyframes de cada uma os instantes em que o desenho troca (os degraus de cada steps()). Ele só
   acorda no próximo desses instantes e só mexe nas animações que trocam ali: a tela é repintada apenas quando algo
   muda, no tempo exato de cada passo. Fora da tela, ou com a janela escondida, ele dorme e a sala não custa nada. */

interface Track {
  /** Instante (relógio do documento) em que a animação começou. */
  born: number;
  delay: number;
  dur: number;
  iters: number;
  /** Pontos de troca dentro de uma volta, em fração de 0 a 1; null = contínua (atualiza a cada tique). */
  pts: number[] | null;
  key: number;
}

/** Junta trocas a menos de SLACK ms numa só repintura (todas andam esse tanto adiantadas, imperceptível). */
const SLACK = 12;
/** Sono máximo (rede de segurança para animações que nasçam sem mudança no DOM). */
const MAX = 1000;
const STEPS = /^steps\((\d+)/;

function track(a: Animation, t: number): Track {
  const timing = a.effect!.getTiming();
  const dur = Number(timing.duration) || 0;
  let pts: number[] | null = [0, 1];
  const kf = (a.effect as KeyframeEffect).getKeyframes();
  for (let i = 0; i + 1 < kf.length && pts; i++) {
    const m = STEPS.exec(kf[i].easing ?? "");
    const o0 = kf[i].computedOffset as number;
    const o1 = kf[i + 1].computedOffset as number;
    if (!m) pts = null;
    else for (let k = 0, n = +m[1]; k <= n; k++) pts.push(o0 + ((o1 - o0) * k) / n);
  }
  if (pts) pts = [...new Set(pts.map((p) => Math.round(p * 1e6) / 1e6))].sort((x, y) => x - y);
  return {
    born: t - ((a.currentTime as number | null) ?? 0),
    delay: Number(timing.delay) || 0,
    dur,
    iters: timing.iterations == null ? Infinity : Number(timing.iterations),
    pts: dur > 0 ? pts : null,
    key: NaN,
  };
}

/** Degrau em que a animação está no tempo local `lt` e quando ele acaba (em tempo local). */
function step(tr: Track, lt: number): [number, number] {
  const at = lt - tr.delay;
  if (at < 0) return [-1, tr.delay];
  if (at >= tr.dur * tr.iters) return [-2, Infinity];
  if (!tr.pts) return [lt, lt]; // contínua: sempre muda
  const it = Math.floor(at / tr.dur);
  const r = (at - it * tr.dur) / tr.dur;
  let i = 0;
  while (i < tr.pts.length && tr.pts[i] <= r) i++;
  const next = i < tr.pts.length ? tr.pts[i] : 1;
  return [it * tr.pts.length + i, tr.delay + (it + next) * tr.dur];
}

/** `fps`: teto de repinturas por segundo (trocas mais próximas que isso saem juntas no quadro seguinte), como a
    pixel art desenhada a 12-15 quadros; `setFps` sobe o teto enquanto a pasta anda, que dá passos mais finos. */
export function officeClock(root: HTMLElement, fps = 15) {
  let min = 1000 / fps;
  const tracks = new WeakMap<Animation, Track>();
  // performance.now e não document.timeline: a linha do tempo do documento para de andar com a janela escondida.
  const now = () => performance.now();
  let timer: ReturnType<typeof setTimeout> | undefined;
  let visible = true;

  const tick = () => {
    clearTimeout(timer);
    timer = undefined;
    if (!visible || document.hidden) return;
    const t = now();
    const T = t + SLACK;
    let wake = Infinity;
    for (const a of root.getAnimations({ subtree: true })) {
      let tr = tracks.get(a);
      if (!tr) tracks.set(a, (tr = track(a, t)));
      const [key, end] = step(tr, T - tr.born);
      if (key !== tr.key) {
        tr.key = key;
        a.currentTime = T - tr.born;
      }
      wake = Math.min(wake, tr.born + end);
    }
    const delay = Math.min(MAX, Math.max(min, wake - SLACK - t));
    timer = setTimeout(tick, delay);
  };
  const sync = () => {
    if (visible && !document.hidden) tick();
    else {
      clearTimeout(timer);
      timer = undefined;
    }
  };

  root.classList.add("clocked");
  const io = new IntersectionObserver((entries) => {
    visible = entries.some((e) => e.isIntersecting);
    sync();
  });
  io.observe(root);
  // Sprites novos (troca de estado, viagem da pasta) nascem pausados no começo: entram no próximo tique. Com a sala
  // fora da tela ou a janela escondida, o relógio dorme, mas anota quando cada um nasceu: ao voltar, a pasta está
  // onde estaria (e acaba junto com o fim da viagem que o componente conta em tempo real), em vez de recomeçar.
  const note = () => {
    const t = now();
    for (const a of root.getAnimations({ subtree: true })) if (!tracks.has(a)) tracks.set(a, track(a, t));
  };
  const mo = new MutationObserver(() => queueMicrotask(visible && !document.hidden ? tick : note));
  mo.observe(root, { childList: true, subtree: true });
  document.addEventListener("visibilitychange", sync);
  sync();

  return {
    setFps(n: number) {
      min = 1000 / n;
      sync();
    },
    destroy() {
      clearTimeout(timer);
      io.disconnect();
      mo.disconnect();
      document.removeEventListener("visibilitychange", sync);
      root.classList.remove("clocked");
    },
  };
}
