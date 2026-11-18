// Imagens coladas em um campo de texto: viram anexos com miniatura.

import type { PastedImage } from "./types";

/** Imagem colada com a miniatura (data URL) para mostrar antes de enviar. */
export type PreviewImage = PastedImage & { preview: string };

let pasteCount = 0;

/** Imagens da área de transferência; quando há alguma, o campo não recebe a colagem como texto. */
export function pastedImageFiles(event: ClipboardEvent): File[] {
  const files = [...(event.clipboardData?.files ?? [])].filter((f) => f.type.startsWith("image/"));
  if (files.length) event.preventDefault();
  return files;
}

/** Lê a imagem em base64; imagens sem nome (prints) ganham "imagem-N". */
export async function readPastedImage(file: File): Promise<PreviewImage> {
  const preview = await new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(file);
  });
  const ext = file.type.split("/")[1]?.replace("jpeg", "jpg") ?? "png";
  pasteCount += 1;
  return { name: file.name || `imagem-${pasteCount}.${ext}`, media_type: file.type, data: preview.split(",")[1] ?? "", preview };
}

/** Limites das imagens de um pedido à esteira (os mesmos do backend). */
export const MAX_PIPELINE_IMAGES = 8;
const MAX_PIPELINE_IMAGE_BYTES = 5 * 1024 * 1024;

/**
 * Junta as imagens coladas às que já estavam, dentro dos limites da esteira; `error` diz o que ficou de fora.
 * Sem imagem na colagem, devolve `null` (o campo recebe o texto normalmente).
 */
export async function addPastedImages(
  event: ClipboardEvent,
  current: PreviewImage[],
): Promise<{ images: PreviewImage[]; error: string | null } | null> {
  const files = pastedImageFiles(event);
  if (!files.length) return null;
  const images = [...current];
  let error: string | null = null;
  for (const file of files) {
    if (images.length >= MAX_PIPELINE_IMAGES) {
      error = `Cole no máximo ${MAX_PIPELINE_IMAGES} imagens.`;
      break;
    }
    if (file.size > MAX_PIPELINE_IMAGE_BYTES) {
      error = "Cada imagem pode ter até 5 MB.";
      continue;
    }
    images.push(await readPastedImage(file));
  }
  return { images, error };
}
