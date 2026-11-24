<script lang="ts">
  import type { Snippet } from "svelte";
  import { accounts } from "../../state/accounts.svelte";
  import { chat } from "../../state/chat.svelte";
  import { stageMessages } from "../../state/pipelines.svelte";
  import {
    choiceFindings, clock, doingPhrase, duration, icon, inputSummary, isUserStep, nextSteps, passesByRole, passLabel, roleIcon, stageAccountId, stageLog,
    stageModelText, STATION, type Glyph,
  } from "../../pipeline";
  import { liveTick } from "../../pipeline-cards.svelte";
  import type { PipelineDetail } from "../../types";
  import type { Transit } from "./PipelineTrack.svelte";

  /** O cartão "Agora": quem trabalha e o que faz, em linguagem de gente; ou a parada que espera você. */
  interface Props {
    detail: PipelineDetail;
    now: number;
    /** O trabalho passando de uma estação a outra (o pacote em movimento). */
    transit: Transit | null;
    /** Botões de decisão do momento: aprovar, escolher, retomar… */
    actions?: Snippet;
    /** Linha curta sob os botões. */
    note?: string | null;
    /** Mostra o entalhe que aponta a estação ativa (só no trilho). */
    notch: boolean;
  }
  let { detail, now, transit, actions, note = null, notch }: Props = $props();

  const passes = $derived(passesByRole(detail));
  const running = $derived(detail.stages.find((s) => s.status === "running" && !isUserStep(s.role)) ?? null);
  const waiting = $derived(detail.stages.findLast((s) => s.status === "running" && isUserStep(s.role)) ?? null);
  const asking = $derived(!!running && !!chat.permissions[running.conversation_id]);

  /** Último comentário do bot: o que escreve agora ou a última mensagem gravada da etapa. */
  const say = $derived.by(() => {
    if (!running) return null;
    const live = chat.turns[running.conversation_id]?.text.trim().split("\n").map((l) => l.replace(/^[#>*\-\s`]+/, "").trim()).findLast(Boolean);
    const text = live || stageLog(stageMessages(running)).findLast((l) => l.verb === "Escreveu")?.target;
    if (!text) return null;
    return text.length > 180 ? `${text.slice(0, 179)}…` : text;
  });

  /** Conta e modelo da etapa em andamento, como no painel. */
  const where = $derived.by(() => {
    if (!running) return "";
    const live = chat.turns[running.conversation_id]?.account ?? null;
    const id = stageAccountId(stageMessages(running));
    const account = accounts.list.find((a) => (live ? a.name === live : a.id === id));
    const model = stageModelText(detail, running, account?.provider ?? "claude");
    return [model, account ? `conta ${account.name}` : null].filter(Boolean).join(" · ");
  });

  type Tick = { stage: string; key: string; glyph: Glyph; verb: string; target: string; title?: string };
  const tick = $derived.by((): Tick | null => {
    if (!running || asking || transit) return null;
    const t = liveTick(running.conversation_id);
    return { stage: running.id, key: `${running.id}-${t.verb}-${t.target}`, glyph: t.glyph, verb: t.verb, target: t.target, title: t.title };
  });
  /** A linha anterior sai por cima quando a próxima entra. */
  let prevTick = $state<Tick | null>(null);
  let shownTick: Tick | null = null;
  $effect(() => {
    if (tick?.key === shownTick?.key) return;
    prevTick = shownTick && tick && shownTick.stage === tick.stage ? shownTick : null;
    shownTick = tick;
  });

  const hero = $derived.by(() => {
    const H = {
      icon: icon("alert", 26, 1.8), eye: "", tag: null as string | null, dim: "", who: "", what: "",
      say: null as string | null, quote: false, io: null as { inLab: string; inText: string; outLab: string; outText: string } | null,
      nexts: [] as ReturnType<typeof nextSteps>,
    };
    if (transit) {
      H.icon = icon(transit.back ? "back" : "doc", 26, 1.8);
      H.eye = "Passando o trabalho";
      H.tag = transit.label;
      H.what = transit.back ? "o trabalho volta para ajustes" : "o trabalho segue adiante";
      H.io = { inLab: "De", inText: transit.from, outLab: "Para", outText: transit.to };
      return H;
    }
    if (detail.status === "awaiting_approval" && waiting) {
      const design = waiting.role === "design_approval";
      const i = passes[waiting.role].indexOf(waiting);
      H.icon = icon("you", 26, 1.8);
      H.eye = "Esperando você";
      H.dim = `há ${duration(now - waiting.started_at)} · a espera não conta no orçamento`;
      H.who = design ? "Aprovação do design:" : "Sua aprovação:";
      H.what = design
        ? (i > 0 ? `o design revisado (v${i + 1}) com o seu ajuste espera a sua decisão` : "o Designer propõe um design")
        : (i > 0 ? `o plano revisado (v${i + 1}) com o seu ajuste espera a sua decisão` : "o Planejador propõe um plano");
      H.say = design
        ? "Abra o design no Claude Design e decida. O plano só começa depois, seguindo o design aprovado."
        : "A esteira está parada: nada é alterado antes da sua decisão, e a espera não conta no orçamento.";
      return H;
    }
    if (detail.status === "awaiting_choice" && waiting) {
      const findings = choiceFindings(waiting);
      const high = findings.filter((f) => /^(alt|high)/i.test(f.severity ?? "")).length;
      H.icon = icon("you", 26, 1.8);
      H.eye = "Esperando você";
      H.dim = `há ${duration(now - waiting.started_at)} · a espera não conta no orçamento`;
      H.who = "Sua escolha:";
      H.what = findings.length
        ? `o Code review trouxe ${findings.length} ${findings.length === 1 ? "achado" : "achados"}${high ? `, ${high} de gravidade alta` : ""}`
        : "o Code review terminou sem a lista de achados";
      H.say = "Marque o que quer corrigir e escolha o caminho dos que têm mais de um. Sem nada marcado, a esteira segue para o Relator.";
      return H;
    }
    if (running) {
      const station = STATION[running.role];
      H.icon = roleIcon(running.role, 26, 1.8);
      H.eye = asking ? "Esperando você" : "Agora";
      H.tag = passLabel(running, passes[running.role].indexOf(running), detail.max_rounds);
      H.dim = [`há ${duration(now - running.started_at)}`, where].filter(Boolean).join(" · ");
      H.who = station.name;
      H.what = asking ? "pede sua autorização para continuar" : doingPhrase(running, detail);
      H.say = asking ? "Responda no painel da etapa, sem abrir a conversa." : (say ?? station.does);
      H.quote = !asking && !!say;
      H.io = { inLab: "Recebeu", inText: inputSummary(running, detail), outLab: "Vai entregar", outText: station.delivers };
      H.nexts = asking ? [] : nextSteps(running.role, detail);
      return H;
    }
    if (detail.status === "running") {
      H.icon = icon("doc", 26, 1.8);
      H.eye = "Agora";
      H.what = detail.stages.length ? "Preparando a próxima etapa…" : "Preparando a primeira etapa…";
      return H;
    }
    if (detail.status === "done") {
      H.icon = icon("check", 26, 1.8);
      H.eye = "Concluída";
      H.what = `Esteira concluída em ${clock(detail.spent_seconds)} de trabalho dos bots`;
      H.say = "Pronto para você conferir: o resultado e a entrega estão logo abaixo.";
      return H;
    }
    if (detail.status === "needs_info") {
      H.icon = icon("alert", 26, 1.8);
      H.eye = "Precisa de informação";
      H.what = "A Triagem achou a task vaga demais para seguir";
      H.say = "Responda às dúvidas logo abaixo para completar o pedido.";
      return H;
    }
    if (detail.status === "cancelled") {
      H.icon = icon("x", 26, 1.8);
      H.eye = "Cancelada";
      H.what = "Execução cancelada por você";
      H.say = detail.mode === "worktree" ? `O que já foi alterado continua na worktree ${detail.branch}.` : "O que já foi alterado continua na pasta de desenvolvimento.";
      return H;
    }
    H.eye = "A esteira parou";
    H.dim = detail.round_limit ? "limite de voltas" : "";
    H.what = detail.round_limit ? `O trabalho foi devolvido ${detail.max_rounds} vezes; a esteira parou para você decidir.` : (detail.note ?? "A esteira parou.");
    H.say = detail.round_limit ? "Ao retomar, a contagem de voltas recomeça e o Implementador corrige o que foi apontado por último." : "Ao retomar, a etapa que parou roda de novo.";
    return H;
  });
  /** Troca de situação: o conteúdo entra de novo, com a animação. */
  const heroKey = $derived(`${detail.status}-${running?.id ?? waiting?.id ?? "x"}-${transit ? `${transit.from}>${transit.to}` : ""}-${asking}`);
</script>

<div class="hero">
  <div class="ava" aria-hidden="true"><span class="ring"></span><span class="orb"></span>{@html hero.icon}</div>
  {#key heroKey}
    <div class="h-main hk">
      <div class="h-eye"><span>{hero.eye}</span>{#if hero.tag}<span class="tg">{hero.tag}</span>{/if}{#if hero.dim}<span class="dim">{hero.dim}</span>{/if}</div>
      <p class="h-title">{#if hero.who}<b>{hero.who}</b>{" "}{/if}<span>{hero.what}</span></p>
      {#if hero.say}<p class="h-say">{@html icon(hero.quote ? "write" : "alert", 14)}<span>{hero.quote ? `“${hero.say}”` : hero.say}</span></p>{/if}
      {#if tick}
        <div class="h-tick" aria-hidden="true">
          {#if prevTick}{#key prevTick.key}<div class="t-line out">{@html icon(prevTick.glyph, 13)}<span class="tv">{prevTick.verb}</span><span class="ta">{prevTick.target}</span></div>{/key}{/if}
          {#key tick.key}<div class="t-line in" title={tick.title}>{@html icon(tick.glyph, 13)}<span class="tv">{tick.verb}</span><span class="ta">{tick.target}</span>{#if tick.title}<span class="tr">{tick.title}</span>{/if}<span class="tl"></span></div>{/key}
        </div>
      {/if}
    </div>
    <div class="h-side hk">
      {#if hero.io}
        <div class="io">
          <div class="io-box"><span class="lab">{hero.io.inLab}</span><span class="v">{hero.io.inText}</span></div>
          <span class="io-arrow" aria-hidden="true">{@html icon("arrow", 16)}</span>
          <div class="io-box out"><span class="lab">{hero.io.outLab}</span><span class="v">{hero.io.outText}</span></div>
        </div>
      {/if}
      {#if hero.nexts.length}
        <div class="nexts">
          {#each hero.nexts as n (n.when)}<span class="nx {n.tone}"><i></i>{n.when}{@html icon("chev", 11)}<b>{n.to}</b></span>{/each}
        </div>
      {/if}
      {#if actions}<div class="h-acts">{@render actions()}</div>{/if}
      {#if note}<p class="h-note">{note}</p>{/if}
    </div>
  {/key}
  {#if notch}<span class="notch" aria-hidden="true"></span>{/if}
</div>

<style>
  .hero { position: relative; z-index: 3; display: grid; grid-template-columns: 56px minmax(0, 1fr) minmax(300px, 420px); align-items: center; gap: 10px 18px; padding: 16px 18px; border: 1px solid var(--hc-line); border-radius: 14px; background: var(--surface); box-shadow: 0 18px 40px -26px var(--hc-glow); transition: border-color 0.5s, box-shadow 0.5s; }
  .hero::before { content: ""; position: absolute; inset: 0; z-index: 0; border-radius: inherit; background: radial-gradient(440px 150px at 0% 50%, var(--hc-bg), transparent 72%); pointer-events: none; }
  .ava, .h-main, .h-side { position: relative; z-index: 1; }
  .ava { width: 56px; height: 56px; display: grid; place-items: center; border: 1px solid var(--hc-line); border-radius: 16px; background: var(--hc-solid); color: var(--hc-text); transition: background-color 0.5s, border-color 0.5s, color 0.5s; }
  .ava :global(svg) { display: block; }
  .orb { position: absolute; inset: -4px; padding: 1.5px; border-radius: 19px; background: conic-gradient(from var(--ang), transparent 0 62%, var(--hc) 84%, transparent 100%); -webkit-mask: linear-gradient(#000 0 0) content-box, linear-gradient(#000 0 0); -webkit-mask-composite: xor; mask: linear-gradient(#000 0 0) content-box, linear-gradient(#000 0 0); mask-composite: exclude; opacity: 0; transition: opacity 0.4s; animation: spin-ang 2.4s linear infinite; }
  .ring { position: absolute; inset: -9px; border: 1.5px solid var(--hc-line); border-radius: 23px; opacity: 0; }
  :global(.tone-live) .orb { opacity: 1; }
  :global(.tone-live) .ring { animation: breathe 2.8s var(--inout) infinite; }
  :global(.tone-you) .ring { animation: breathe 1.8s var(--inout) infinite; }
  .h-main { min-width: 0; }
  .hk { animation: hero-in 0.55s var(--ease) both; }
  .h-eye { display: flex; align-items: center; flex-wrap: wrap; gap: 4px 10px; font-size: 11px; font-weight: 700; letter-spacing: 0.07em; text-transform: uppercase; color: var(--hc-text); }
  .tg { padding: 0 7px; border: 1px solid var(--hc-line); border-radius: 6px; font: 500 10.5px/1.7 var(--mono); letter-spacing: 0; text-transform: none; color: var(--text-2); }
  .dim { font-weight: 500; letter-spacing: 0; text-transform: none; color: var(--muted); }
  .h-title { margin: 4px 0 0; font: 600 19px/1.3 var(--display); letter-spacing: -0.012em; color: var(--text); text-wrap: pretty; }
  .h-title b { font-weight: 700; color: var(--hc-text); }
  .h-say { display: flex; gap: 8px; min-width: 0; margin: 6px 0 0; font-size: 13px; line-height: 1.5; color: var(--text-2); overflow-wrap: anywhere; }
  .h-say :global(svg) { flex: none; margin-top: 3px; color: var(--muted); }
  .h-tick { position: relative; height: 28px; margin-top: 9px; overflow: hidden; border: 1px solid var(--border); border-radius: 8px; background: var(--surface-2); }
  .t-line { position: absolute; inset: 0; display: flex; align-items: center; gap: 8px; min-width: 0; padding: 0 10px; font-size: 12px; white-space: nowrap; }
  .t-line.in { animation: tick-in 0.5s var(--ease) both; }
  .t-line.out { animation: tick-out 0.5s var(--ease) both; }
  .t-line :global(svg) { flex: none; color: var(--hc-text); }
  .tv { flex: none; font-weight: 600; color: var(--text); }
  .ta { flex: 0 1 auto; min-width: 0; overflow: hidden; text-overflow: ellipsis; font: 400 11.5px var(--mono); color: var(--text-2); }
  .tr { flex: 1 1 0; min-width: 0; overflow: hidden; text-overflow: ellipsis; text-align: right; font: 400 10.5px var(--mono); color: var(--muted); }
  .tl { flex: none; margin-left: auto; width: 7px; height: 7px; border-radius: 50%; background: var(--hc); animation: blink 1.2s ease-in-out infinite; }
  .tr + .tl { margin-left: 0; }
  .h-side { min-width: 0; display: flex; flex-direction: column; gap: 10px; }
  .io { display: grid; grid-template-columns: minmax(0, 1fr) 30px minmax(0, 1fr); align-items: stretch; }
  .io-box { min-width: 0; padding: 8px 10px; border: 1px solid var(--border); border-radius: 10px; background: var(--surface-2); }
  .io-box .lab { display: block; margin-bottom: 2px; font-size: 10px; font-weight: 600; letter-spacing: 0.05em; text-transform: uppercase; color: var(--muted); }
  .io-box .v { display: block; font-size: 12.5px; line-height: 1.4; color: var(--text); text-wrap: pretty; }
  .io-box.out { border-color: var(--hc-line); }
  .io-arrow { position: relative; display: grid; place-items: center; color: var(--hc-text); }
  .io-arrow::before { content: ""; position: absolute; left: 3px; right: 3px; top: 50%; height: 2px; margin-top: -1px; background: repeating-linear-gradient(90deg, var(--hc-line) 0 4px, transparent 4px 8px); background-size: 16px 2px; animation: dash 0.9s linear infinite; }
  .io-arrow :global(svg) { position: relative; padding: 2px; border-radius: 50%; background: var(--surface); }
  .nexts { display: flex; flex-wrap: wrap; gap: 6px; }
  .nx { display: inline-flex; align-items: center; gap: 6px; max-width: 100%; padding: 3px 10px; border: 1px solid var(--border); border-radius: 999px; background: var(--bg-side); font-size: 11.5px; color: var(--text-2); }
  .nx b { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 600; color: var(--text); }
  .nx i { flex: none; width: 6px; height: 6px; border-radius: 50%; background: var(--muted); }
  .nx :global(svg) { flex: none; color: var(--faint); }
  .nx.ok i { background: var(--ok); }
  .nx.bad i { background: var(--bad); }
  .nx.you i { background: var(--warn); }
  .h-acts { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 8px; }
  .h-acts :global(.btn) { min-height: 38px; }
  .h-note { margin: 0; font-size: 11.5px; color: var(--muted); text-align: right; }
  .notch { position: absolute; z-index: 2; bottom: -7px; left: calc((var(--bi, 0) + 0.5) * 12.5%); width: 12px; height: 12px; margin-left: -6px; border-right: 1px solid var(--hc-line); border-bottom: 1px solid var(--hc-line); background: var(--surface); transform: rotate(45deg); transition: left var(--bms, 700ms) var(--inout), border-color 0.5s; }

  @keyframes spin-ang { to { --ang: 360deg; } }
  @keyframes breathe { 0%, 100% { transform: scale(0.96); opacity: 0; } 50% { transform: scale(1.06); opacity: 0.9; } }
  @keyframes hero-in { from { opacity: 0; transform: translateY(6px); filter: blur(3px); } to { opacity: 1; transform: none; filter: none; } }
  @keyframes tick-in { from { opacity: 0; transform: translateY(100%); } to { opacity: 1; transform: none; } }
  @keyframes tick-out { from { opacity: 1; transform: none; } to { opacity: 0; transform: translateY(-100%); } }
  @keyframes blink { 50% { opacity: 0.35; } }
  @keyframes dash { to { background-position: 16px 0; } }

  @media (max-width: 1100px) {
    .notch { display: none; }
  }
  @media (max-width: 980px) {
    .hero { grid-template-columns: 48px minmax(0, 1fr); }
    .h-side { grid-column: 1 / -1; }
    .ava { width: 48px; height: 48px; }
    .h-acts { justify-content: flex-start; }
    .h-note { text-align: left; }
  }
</style>
