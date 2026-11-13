// Rascunhos do campo de mensagem, por conversa. Ficam fora do componente do chat
// para sobreviver quando o usuário vai para outra tela e volta.

import type { PastedImage } from "../types";

export interface Draft {
  text: string;
  attachments: string[];
  pasted: (PastedImage & { preview: string })[];
}

/** Chave da conversa nova, que ainda não tem id. */
export const NEW_CONVERSATION = "__nova__";

const drafts = new Map<string, Draft>();

export function readDraft(key: string): Draft {
  const d = drafts.get(key);
  return d ? { text: d.text, attachments: [...d.attachments], pasted: [...d.pasted] } : { text: "", attachments: [], pasted: [] };
}

export function saveDraft(key: string, draft: Draft): void {
  if (!draft.text && !draft.attachments.length && !draft.pasted.length) drafts.delete(key);
  else drafts.set(key, { text: draft.text, attachments: [...draft.attachments], pasted: [...draft.pasted] });
}
