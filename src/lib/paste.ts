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
