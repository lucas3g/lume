// O que o usuário marcou nas conversas: ações feitas nos cartões "Ação sua" e mensagens fixadas.
// Fica guardado no aplicativo (localStorage), por conversa e mensagem.

const KEY = "lume.marcas.v1";

interface Marks {
  /** "conversa:mensagem:posição" das ações feitas. */
  done: Record<string, true>;
  /** "conversa:mensagem" das mensagens fixadas. */
  pins: Record<string, true>;
}

function load(): Marks {
  try {
    const value = JSON.parse(localStorage.getItem(KEY) ?? "null");
    if (value && typeof value.done === "object" && typeof value.pins === "object") return value;
  } catch {
    // sem nada guardado, ou guardado de outro jeito: começa vazio
  }
  return { done: {}, pins: {} };
}

export const marks = $state<Marks>(load());

function save() {
  try {
    localStorage.setItem(KEY, JSON.stringify(marks));
  } catch {
    // sem espaço ou sem acesso: as marcas valem só até fechar o app
  }
}

export const taskKey = (conversation: string, message: number, index: number) => `${conversation}:${message}:${index}`;
const pinKey = (conversation: string, message: number) => `${conversation}:${message}`;

export function toggleTask(key: string): void {
  if (marks.done[key]) delete marks.done[key];
  else marks.done[key] = true;
  save();
}

export function isPinned(conversation: string | null, message: number): boolean {
  return !!conversation && !!marks.pins[pinKey(conversation, message)];
}

export function togglePin(conversation: string, message: number): void {
  const key = pinKey(conversation, message);
  if (marks.pins[key]) delete marks.pins[key];
  else marks.pins[key] = true;
  save();
}
