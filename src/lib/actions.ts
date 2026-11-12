// Comportamentos reutilizáveis de elementos (ações do Svelte).

/** Chama `onclose` ao clicar fora do elemento ou apertar Esc. */
export function dismissable(node: HTMLElement, onclose: () => void) {
  const onPointer = (event: PointerEvent) => {
    if (!node.contains(event.target as Node)) onclose();
  };
  const onKey = (event: KeyboardEvent) => {
    if (event.key === "Escape") onclose();
  };
  document.addEventListener("pointerdown", onPointer, true);
  document.addEventListener("keydown", onKey);
  return {
    destroy() {
      document.removeEventListener("pointerdown", onPointer, true);
      document.removeEventListener("keydown", onKey);
    },
  };
}
