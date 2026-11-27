// O que os cartões de destaque fazem dentro de uma resposta já desenhada: marcar as ações do
// usuário, contar quantas faltam, propor resolver o que ficou de fora e copiar trechos de código.

import { untrack } from "svelte";
import { confetti } from "./confetti";
import { marks, taskKey, toggleTask } from "./state/marks.svelte";

export interface CalloutOptions {
  conversation: string | null;
  /** Mensagem gravada (id > 0); sem ela, as ações ainda não podem ser marcadas. */
  message: number;
  /** Conteúdo da resposta: quando muda, o HTML foi refeito e os controles voltam a ser montados. */
  content: string;
  /** Põe um texto no campo de mensagem. */
  onfill: (text: string) => void;
}

const CHECK = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12.5l4.5 4.5L19 7.5"/></svg>';

/** Itens das listas de primeiro nível dos cartões "Ação sua", na mesma ordem de `actionsIn`. */
const taskItems = (root: HTMLElement) =>
  [...root.querySelectorAll<HTMLElement>(":scope > .callout.act > .co-body > ul > li, :scope > .callout.act > .co-body > ol > li")];

/** Anexo (`{@attach}`) da área da resposta. */
export function callouts(options: CalloutOptions) {
  return (root: HTMLElement) => {
    void options.content;
    const { conversation, message } = options;
    const markable = !!conversation && message > 0;
    const cleanup: (() => void)[] = [];

    // As marcas são lidas sem criar dependência: o clique atualiza só o item, e o ✓ anima.
    const items = taskItems(root);
    const keyOf = (index: number) => taskKey(conversation!, message, index);
    const isDone = (index: number) => untrack(() => !!marks.done[keyOf(index)]);
    const boxes: HTMLButtonElement[] = [];
    const counters: { el: HTMLElement; from: number; to: number }[] = [];

    const paint = () => {
      items.forEach((li, index) => {
        const done = isDone(index);
        li.classList.toggle("done", done);
        boxes[index]?.setAttribute("aria-checked", String(done));
        boxes[index]?.setAttribute("aria-label", done ? "Marcar como não feita" : "Marcar como feita");
      });
      for (const c of counters) {
        let done = 0;
        for (let i = c.from; i < c.to; i++) if (isDone(i)) done++;
        c.el.textContent = `${done}/${c.to - c.from}`;
        c.el.classList.toggle("all", done === c.to - c.from);
      }
    };

    items.forEach((li, index) => {
      li.querySelector(":scope > input[type=checkbox]")?.remove();
      li.classList.add("task");
      if (!markable) return;
      const box = document.createElement("button");
      box.type = "button";
      box.className = "task-box";
      box.setAttribute("role", "checkbox");
      box.innerHTML = CHECK;
      box.onclick = () => {
        toggleTask(keyOf(index));
        paint();
        const callout = li.closest<HTMLElement>(".callout");
        if (callout && isDone(index) && items.every((_, i) => isDone(i))) confetti(callout);
      };
      li.prepend(box);
      boxes[index] = box;
      cleanup.push(() => box.remove());
    });

    // Contador "feitas/total" no cabeçalho de cada cartão de ações.
    if (markable) {
      let offset = 0;
      root.querySelectorAll<HTMLElement>(":scope > .callout.act").forEach((callout) => {
        const own = items.filter((li) => li.closest(".callout") === callout).length;
        const from = offset;
        offset += own;
        if (!own) return;
        const el = document.createElement("span");
        el.className = "co-count";
        el.title = "Ações feitas";
        callout.querySelector(".co-head")?.append(el);
        counters.push({ el, from, to: offset });
        cleanup.push(() => el.remove());
      });
    }
    paint();

    // "Ficou fora": oferece pedir a correção agora.
    root.querySelectorAll<HTMLElement>(".callout.note > .co-body").forEach((body) => {
      const title = body.parentElement?.querySelector(".co-t")?.textContent?.trim() ?? "";
      const button = document.createElement("button");
      button.type = "button";
      button.className = "co-action";
      button.textContent = "Resolver agora";
      button.onclick = () => options.onfill(title ? `Resolva agora o que ficou de fora: ${title}.` : "Resolva agora o que ficou de fora.");
      body.append(button);
      cleanup.push(() => button.remove());
    });

    // Clique num trecho de código copia (se não estiver selecionando texto).
    const onclick = (event: MouseEvent) => {
      const code = (event.target as HTMLElement).closest("code");
      if (!code || code.closest("pre") || !root.contains(code)) return;
      if (!window.getSelection()?.isCollapsed) return;
      navigator.clipboard?.writeText(code.textContent ?? "").then(
        () => {
          code.classList.remove("copied");
          void code.offsetWidth;
          code.classList.add("copied");
          setTimeout(() => code.classList.remove("copied"), 1200);
        },
        () => {},
      );
    };
    root.addEventListener("click", onclick);
    cleanup.push(() => root.removeEventListener("click", onclick));

    return () => cleanup.forEach((fn) => fn());
  };
}
