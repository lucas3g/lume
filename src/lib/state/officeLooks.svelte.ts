// Visual dos bonecos do escritório: carregado uma vez e aplicado ao desenho (scene.ts).

import { api } from "../api";
import { setOfficeLooks } from "../office/scene";
import type { OfficeLooks } from "../types";

/** `version` cresce a cada troca: quem desenha bonecos depende dela para refazer a vista. */
export const officeLooks = $state<{ looks: OfficeLooks; version: number; loaded: boolean }>({ looks: {}, version: 0, loaded: false });

function apply(looks: OfficeLooks) {
  setOfficeLooks(looks);
  officeLooks.looks = looks;
  officeLooks.version += 1;
}

let loading: Promise<void> | null = null;
/** Carrega uma vez; chamadas seguintes esperam a mesma carga. */
export function loadOfficeLooks(): Promise<void> {
  loading ??= api.officeLooks().then(
    (looks) => {
      apply(looks);
      officeLooks.loaded = true;
    },
    () => {
      // Sem os visuais salvos, o escritório aparece com os bonecos padrão (e a próxima carga tenta de novo).
      officeLooks.loaded = true;
      loading = null;
    },
  );
  return loading;
}

/** Salva o mapa inteiro e aplica o que o núcleo devolveu (já normalizado). */
export async function saveOfficeLooks(looks: OfficeLooks): Promise<void> {
  apply(await api.saveOfficeLooks(looks));
}
