/* Bonecos chibi do escritório em desenho vetorial liso (usados por scene.ts e pela prévia do editor).
   Cabeça grande (metade da altura), olhos com íris e brilho, sobrancelha grossa, boca por expressão, barba, tênis
   brancos, traço escuro de nanquim e sombra chapada em 2 tons com a luz vindo do alto à esquerda, também no boneco
   espelhado. Módulo puro: visual -> peças em SVG (strings), sem DOM e sem estado global.

   Coordenadas: px lógicos da cena (360x186, exibida a ~3x), relativas ao quadril, com y para baixo. Cada peça é um
   <g> nesse sistema; quem monta só translada até o quadril (o assento da mesa ou o ponto de quem anda).
   Tamanho: de pé, 14x24 px lógicos (x de -7 a 7, y de -18 a +6), do topo do cabelo com o traço à
   sola, com os pés na última linha; cachos, coque e gorro passam até 2,3 px do topo (o SVG da cena não recorta).
   Sentado: 14x18 acima do quadril (-18 a 0) e o colo, que desce até +6,2 para a mesa cobrir. A cabeça tem metade da
   altura e o centro do crânio fica em (-0,3; -10,9).

   Peças, na ordem de pintura de uma pessoa: sombra e pernas, corpo (tronco + cabeça, já com as pálpebras fechadas),
   olhos (abertos ou felizes ^ ^; esconder a camada aberta é a piscada), boca (por expressão) e braços (por pose).
   Os óculos saem no corpo e de novo nos olhos, iguais: a piscada não os faz sumir.
   Sombra chapada: cada parte é pintada na cor da sombra e, por cima, na cor base deslocada na direção da luz, tudo
   recortado na própria forma (clipPath): sobra uma meia-lua de sombra do lado oposto à luz. No espelho o
   deslocamento troca de lado no espaço local, e a luz segue vindo da esquerda da tela.
   Cada forma é definida uma vez (<path id> em <defs>) e reaproveitada com <use>. Os ids vêm do conteúdo (hash) com o
   prefixo `ids`: definições repetidas em sprites diferentes são idênticas e inofensivas, e o <use> de dentro do
   recorte tem visibility="visible", para um quadro escondido (visibility: hidden) não esvaziar o recorte de outro. */

// ---- tipos públicos

/** Visual de um boneco: o BotLookShape do scene.ts mais a barba (e, opcional, a cor dela). */
export interface ChibiLook {
  skin: string;
  skin_shade?: string;
  hair: string;
  hair_light?: string;
  shirt: string;
  shirt_shade?: string;
  pants: string;
  hair_style: string;
  accessory: string | null;
  outfit: string;
  beard?: string;
  /** Cor da barba (padrão: a do cabelo). Ex.: barba acobreada com cabelo castanho. */
  beard_color?: string;
}

export type ChibiView = "front" | "back";
export type ChibiEyes = "open" | "happy";
export type ChibiMouth = "neutral" | "smile" | "grin" | "grimace" | "ohh";
/** Quadros da caminhada (0..3) ou parado. */
export type ChibiStep = 0 | 1 | 2 | 3 | "stand";
/** Braços por pose (os nomes do POSE do scene.ts). As de dois braços desenham os dois; as outras, só aquele. */
export type ChibiPose =
  | "hang" | "restL" | "restR" | "crossL" | "crossR" | "sipR" | "palmL"
  | "stretch0" | "stretch1" | "wave0" | "wave1" | "fold";

export interface ChibiOpts {
  /** Espelha o desenho (olhando para sudeste ou nordeste). A luz continua vindo do alto à esquerda da tela. */
  flip?: boolean;
  /** Prefixo dos ids (padrão "cb"). Um por sprite deixa cada sprite com as suas próprias definições. */
  ids?: string;
  /** false: não calcula as células da silhueta (quem não precisa de máscara de clique, como quem anda). */
  cells?: boolean;
}

export interface ChibiPiece {
  /** Um <g> em px lógicos relativos ao quadril (já espelhado, se flip). */
  svg: string;
  /** Células de px lógico ocupadas pela silhueta, com o traço: [x, y] cobre [x, x+1) x [y, y+1). Olhos e boca não
      têm (ficam dentro da cabeça). */
  cells: Array<[number, number]>;
  /** Caixa [x, y, w, h] do desenho com o traço, relativa ao quadril. */
  box: [number, number, number, number];
}

// ---- números, cores e formas

type Pt = [number, number];
type Cmd = [string, ...number[]];
interface Shape { cmds: Cmd[]; poly: Pt[]; d?: string }

/** Número com até 2 casas, sem zero à esquerda (".5", "-.25"). */
const f = (v: number): string => {
  const r = Math.round(v * 100) / 100;
  return (r === 0 ? 0 : r).toString().replace(/^(-?)0\./, "$1.");
};
const rgb = (c: string): number[] => [1, 3, 5].map((i) => parseInt(c.slice(i, i + 2), 16));
const hex = (a: number[]): string =>
  "#" + a.map((v) => Math.max(0, Math.min(255, Math.round(v))).toString(16).padStart(2, "0")).join("");
const mix = (a: string, b: string, t: number): string => {
  const p = rgb(a), q = rgb(b);
  return hex(p.map((v, i) => v + (q[i] - v) * t));
};
const mul = (a: string, b: string): string => {
  const p = rgb(a), q = rgb(b);
  return hex(p.map((v, i) => (v * q[i]) / 255));
};
const luma = (c: string): number => {
  const p = rgb(c);
  return (0.299 * p[0] + 0.587 * p[1] + 0.114 * p[2]) / 255;
};

const INK = "#1d1418";
/** Traço interno; o contorno de fora ganha OUT a mais de cada lado (sai mais grosso que os de dentro). */
const LINE = 0.32;
const OUT = 0.24;
const WHITE = "#f7f4ef";

const dOf = (s: Shape, dx = 0, dy = 0): string =>
  dx || dy
    ? s.cmds.map((c) => c[0] + c.slice(1).map((v, i) => f((v as number) + (i % 2 ? dy : dx))).join(" ")).join("")
    : s.d || (s.d = s.cmds.map((c) => c[0] + c.slice(1).map((v) => f(v as number)).join(" ")).join(""));

function cubic(p0: Pt, c1: Pt, c2: Pt, p1: Pt, out: Pt[]): void {
  for (let i = 1; i <= 8; i++) {
    const t = i / 8, u = 1 - t, a = u * u * u, b = 3 * u * u * t, c = 3 * u * t * t, d = t * t * t;
    out.push([a * p0[0] + b * c1[0] + c * c2[0] + d * p1[0], a * p0[1] + b * c1[1] + c * c2[1] + d * p1[1]]);
  }
}
/** Nó de curva: [x, y] liso ou [x, y, 1] com quina. */
type Node = readonly [number, number] | readonly [number, number, number];
/** Curva lisa (Catmull-Rom) pelos nós; fechada por padrão. */
function spline(nodes: readonly Node[], closed = true): Shape {
  const n = nodes.length;
  const P = (i: number): Node => (closed ? nodes[(i + n) % n] : nodes[Math.max(0, Math.min(n - 1, i))]);
  const cmds: Cmd[] = [["M", nodes[0][0], nodes[0][1]]];
  const poly: Pt[] = [[nodes[0][0], nodes[0][1]]];
  for (let i = 0; i < (closed ? n : n - 1); i++) {
    const p0 = P(i - 1), p1 = P(i), p2 = P(i + 1), p3 = P(i + 2);
    const c1: Pt = p1[2]
      ? [p1[0] + (p2[0] - p1[0]) * 0.3, p1[1] + (p2[1] - p1[1]) * 0.3]
      : [p1[0] + (p2[0] - p0[0]) / 6, p1[1] + (p2[1] - p0[1]) / 6];
    const c2: Pt = p2[2]
      ? [p2[0] - (p2[0] - p1[0]) * 0.3, p2[1] - (p2[1] - p1[1]) * 0.3]
      : [p2[0] - (p3[0] - p1[0]) / 6, p2[1] - (p3[1] - p1[1]) / 6];
    cmds.push(["C", c1[0], c1[1], c2[0], c2[1], p2[0], p2[1]]);
    cubic([p1[0], p1[1]], c1, c2, [p2[0], p2[1]], poly);
  }
  if (closed) cmds.push(["Z"]);
  return { cmds, poly };
}
/** Elipse (quatro cúbicas), girada em graus. */
function ell(cx: number, cy: number, rx: number, ry: number, rot = 0): Shape {
  const a = (rot * Math.PI) / 180, ca = Math.cos(a), sa = Math.sin(a), K = 0.5523;
  const T = (x: number, y: number): Pt => [cx + x * ca - y * sa, cy + x * sa + y * ca];
  const q: Pt[] = [T(rx, 0), T(0, ry), T(-rx, 0), T(0, -ry)];
  const h: Pt[][] = [
    [T(rx, ry * K), T(rx * K, ry)], [T(-rx * K, ry), T(-rx, ry * K)],
    [T(-rx, -ry * K), T(-rx * K, -ry)], [T(rx * K, -ry), T(rx, -ry * K)],
  ];
  const cmds: Cmd[] = [["M", q[0][0], q[0][1]]], poly: Pt[] = [q[0]];
  for (let i = 0; i < 4; i++) {
    const p1 = q[(i + 1) % 4];
    cmds.push(["C", h[i][0][0], h[i][0][1], h[i][1][0], h[i][1][1], p1[0], p1[1]]);
    cubic(q[i], h[i][0], h[i][1], p1, poly);
  }
  cmds.push(["Z"]);
  return { cmds, poly };
}
/** Polígono de trechos retos. */
function pgon(pts: readonly Pt[]): Shape {
  const cmds: Cmd[] = pts.map((p, i) => [i ? "L" : "M", p[0], p[1]] as Cmd);
  cmds.push(["Z"]);
  return { cmds, poly: pts.map((p) => [p[0], p[1]] as Pt) };
}
/** Linha quebrada aberta. */
function polyline(pts: readonly Pt[]): Shape {
  return { cmds: pts.map((p, i) => [i ? "L" : "M", p[0], p[1]] as Cmd), poly: pts.map((p) => [p[0], p[1]] as Pt) };
}
/** Forma deslocada (a mesma forma em outro ponto). */
function moved(s: Shape, dx: number, dy: number): Shape {
  return {
    cmds: s.cmds.map((c) => [c[0], ...c.slice(1).map((v, i) => (v as number) + (i % 2 ? dy : dx))] as Cmd),
    poly: s.poly.map((p) => [p[0] + dx, p[1] + dy] as Pt),
  };
}
/** Curva aberta pelos nós (traços de rosto, fios). */
const curve = (nodes: readonly Node[]): Shape => spline(nodes, false);

function inPoly(poly: Pt[], x: number, y: number): boolean {
  let inside = false;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const a = poly[i], b = poly[j];
    if (a[1] > y !== b[1] > y && x < ((b[0] - a[0]) * (y - a[1])) / (b[1] - a[1]) + a[0]) inside = !inside;
  }
  return inside;
}
function segDist(x: number, y: number, a: Pt, b: Pt): number {
  const dx = b[0] - a[0], dy = b[1] - a[1], l2 = dx * dx + dy * dy || 1;
  const t = Math.max(0, Math.min(1, ((x - a[0]) * dx + (y - a[1]) * dy) / l2));
  return Math.hypot(x - a[0] - dx * t, y - a[1] - dy * t);
}
function polyDist(poly: Pt[], x: number, y: number, closed: boolean): number {
  let d = 1e9;
  const n = poly.length;
  for (let i = 0; i + 1 < n + (closed ? 1 : 0); i++) d = Math.min(d, segDist(x, y, poly[i], poly[(i + 1) % n]));
  return d;
}
/** Hash do conteúdo (dois de 32 bits: djb2 e FNV-1a) para os ids: formas diferentes não dividem um id. */
function hashStr(s: string): string {
  let h = 5381, g = 0x811c9dc5;
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    h = (Math.imul(h, 33) ^ c) | 0;
    g = Math.imul(g ^ c, 0x01000193);
  }
  return (h >>> 0).toString(36) + (g >>> 0).toString(36);
}

// ---- paleta derivada do visual

interface Pal {
  skin: string; skinSh: string; skinDk: string; blush: string;
  hair: string; hairSh: string; hairLt: string; brow: string; fade: string;
  shirt: string; shirtSh: string; pants: string; pantsSh: string;
  beard: string; beardSh: string;
}
function palette(l: ChibiLook): Pal {
  const skinSh = l.skin_shade || mix(l.skin, mul(l.skin, "#c98f80"), 0.85);
  const beard = l.beard_color || l.hair, light = luma(l.hair) > 0.5;
  return {
    skin: l.skin, skinSh, skinDk: mix(skinSh, "#4a1f1a", 0.4), blush: mix(l.skin, "#ff5f66", 0.38),
    hair: l.hair, hairSh: mix(l.hair, "#0c0a12", light ? 0.28 : 0.42),
    hairLt: l.hair_light || mix(l.hair, "#fff6ea", 0.3), brow: mix(l.hair, INK, light ? 0.4 : 0.6),
    fade: mix(l.hair, skinSh, 0.62),
    shirt: l.shirt, shirtSh: l.shirt_shade || mix(l.shirt, "#141020", 0.3),
    pants: l.pants, pantsSh: mix(l.pants, "#0a0812", 0.35),
    beard, beardSh: mix(beard, "#0c0a12", 0.3),
  };
}

// ---- contexto de desenho de uma peça

interface Ctx {
  /** Lado da luz no espaço local: -1 = esquerda (sem espelho), +1 no espelho (que vira esquerda na tela). */
  side: number;
  flip: boolean;
  ids: string;
  cells: boolean;
  defs: Map<string, string>;
  back: string[];
  out: string[];
  hits: Array<(x: number, y: number) => boolean>;
  /** Caixa da silhueta (para as células) e de tudo o que foi desenhado (para a caixa da peça). */
  bb: [number, number, number, number];
  vb: [number, number, number, number];
}
function ctx(o: ChibiOpts | undefined): Ctx {
  const flip = !!(o && o.flip);
  return {
    flip, side: flip ? 1 : -1, ids: (o && o.ids) || "cb", cells: !(o && o.cells === false), defs: new Map(), back: [], out: [], hits: [],
    bb: [1e9, 1e9, -1e9, -1e9], vb: [1e9, 1e9, -1e9, -1e9],
  };
}
function grow(b: [number, number, number, number], poly: readonly Pt[], pad: number): void {
  for (const p of poly) {
    b[0] = Math.min(b[0], p[0] - pad); b[1] = Math.min(b[1], p[1] - pad);
    b[2] = Math.max(b[2], p[0] + pad); b[3] = Math.max(b[3], p[1] + pad);
  }
}
/** Define a forma uma vez (<path id>) e devolve o id. */
function ref(c: Ctx, s: Shape): string {
  const d = dOf(s), id = c.ids + hashStr(d);
  if (!c.defs.has(id)) c.defs.set(id, `<path id="${id}" d="${d}"/>`);
  return id;
}
const use = (id: string, attrs: string): string => `<use href="#${id}"${attrs}/>`;
/** Recorte pela forma (deslocada de dx, dy, se pedido). */
function clipOf(c: Ctx, s: Shape, dx = 0, dy = 0): string {
  const pid = ref(c, s), id = pid + (dx || dy ? "s" + hashStr(f(dx) + f(dy)) : "c");
  if (!c.defs.has(id)) {
    const tr = dx || dy ? ` transform="translate(${f(dx)} ${f(dy)})"` : "";
    c.defs.set(id, `<clipPath id="${id}">${use(pid, ` visibility="visible"${tr}`)}</clipPath>`);
  }
  return id;
}
/** A forma entra na silhueta: contorno grosso por fora, máscara de clique e caixa. */
function solid(c: Ctx, s: Shape, id: string): void {
  c.back.push(use(id, ""));
  const poly = s.poly, pad = OUT + LINE / 2, b: [number, number, number, number] = [1e9, 1e9, -1e9, -1e9];
  grow(b, poly, pad);
  c.hits.push((x, y) => x >= b[0] && x <= b[2] && y >= b[1] && y <= b[3] && (inPoly(poly, x, y) || polyDist(poly, x, y, true) <= pad));
  grow(c.bb, poly, pad);
}
interface PartOpt {
  /** Deslocamento [x, y] da forma em direção à luz: a largura da meia-lua de sombra. */
  off?: readonly [number, number];
  /** Recortado na forma, por cima das cores (estampa, sombras projetadas, detalhes). */
  inner?: string;
  /** Traço (0: sem traço). */
  line?: number;
  ink?: string;
  /** Fica fora da silhueta (contorno grosso e clique): detalhes por dentro de outra parte. */
  loose?: boolean;
  /** Estampa: [na sombra, na luz] no lugar das cores chapadas. */
  print?: [string, string];
}
/** Parte com sombra chapada: cor da sombra, cor base deslocada para a luz (recortadas na forma) e traço. */
function part(c: Ctx, s: Shape, base: string, shade: string, o: PartOpt = {}): void {
  const off = o.off || [0.85, 0.7], id = ref(c, s), cid = clipOf(c, s), dx = c.side * off[0], dy = -off[1];
  const fill = o.print
    ? o.print[0] + `<g clip-path="url(#${clipOf(c, s, dx, dy)})">${o.print[1]}</g>`
    : use(id, ` fill="${shade}"`) + use(id, ` fill="${base}" transform="translate(${f(dx)} ${f(dy)})"`);
  c.out.push(`<g clip-path="url(#${cid})">${fill}${o.inner || ""}</g>`);
  const w = o.line === undefined ? LINE : o.line;
  if (w) c.out.push(use(id, ` fill="none" stroke="${o.ink || INK}" stroke-width="${f(w)}"`));
  if (!o.loose) solid(c, s, id);
  grow(c.vb, s.poly, w / 2 + (o.loose ? 0 : OUT));
}
/** Forma lisa, sem sombra (detalhes), com traço opcional. */
function flat(c: Ctx, s: Shape, fill: string, w = 0, ink = INK): void {
  c.out.push(`<path d="${dOf(s)}" fill="${fill}"${w ? ` stroke="${ink}" stroke-width="${f(w)}"` : ""}/>`);
  grow(c.vb, s.poly, w / 2);
}
/** Traço aberto (fios, linhas de expressão). */
function stroke(c: Ctx, s: Shape, w: number, color = INK): void {
  c.out.push(`<path d="${dOf(s)}" fill="none" stroke="${color}" stroke-width="${f(w)}"/>`);
  grow(c.vb, s.poly, w / 2);
}
/** Membros como traço grosso de pontas redondas: contorno, cor da sombra e cor base deslocada para a luz (faixa de
    sombra do lado oposto). O contorno de todos os trechos sai antes, para as juntas (cotovelo, manga) não ganharem linha. */
interface Seg { pts: Pt[]; w: number; base: string; shade: string }
function limbs(c: Ctx, segs: Seg[]): void {
  const ids = segs.map((s) => ref(c, polyline(s.pts)));
  segs.forEach((s, i) => {
    c.out.push(use(ids[i], ` fill="none" stroke="${INK}" stroke-width="${f(s.w + 2 * LINE)}"`));
    c.back.push(use(ids[i], ` fill="none" stroke-width="${f(s.w + 2 * LINE + 2 * OUT)}"`));
    const pts = s.pts, r = s.w / 2 + LINE + OUT, b: [number, number, number, number] = [1e9, 1e9, -1e9, -1e9];
    grow(b, pts, r);
    c.hits.push((x, y) => x >= b[0] && x <= b[2] && y >= b[1] && y <= b[3] && polyDist(pts, x, y, false) <= r);
    grow(c.bb, pts, r);
    grow(c.vb, pts, r);
  });
  segs.forEach((s, i) => {
    const d = s.w * 0.16;
    c.out.push(use(ids[i], ` fill="none" stroke="${s.shade}" stroke-width="${f(s.w)}"`));
    c.out.push(use(ids[i], ` fill="none" stroke="${s.base}" stroke-width="${f(s.w - 2.2 * d)}" transform="translate(${f(c.side * d * 0.8)} ${f(-d)})"`));
  });
}
const box4 = (b: number[], flip: boolean): [number, number, number, number] => {
  if (b[0] > b[2]) return [0, 0, 0, 0];
  const r = flip ? [-b[2], b[1], b[2] - b[0], b[3] - b[1]] : [b[0], b[1], b[2] - b[0], b[3] - b[1]];
  return r.map((v) => Math.round(v * 100) / 100) as [number, number, number, number];
};
/** Fecha a peça: defs, contorno grosso por baixo, desenho; espelho; células e caixa. */
function finish(c: Ctx): ChibiPiece {
  const defs = c.defs.size ? `<defs>${[...c.defs.values()].join("")}</defs>` : "";
  const back = c.back.length ? `<g fill="${INK}" stroke="${INK}" stroke-width="${f(2 * OUT + LINE)}">${c.back.join("")}</g>` : "";
  const body = defs + back + c.out.join("");
  const svg = `<g shape-rendering="geometricPrecision" stroke-linejoin="round" stroke-linecap="round">${c.flip ? `<g transform="scale(-1 1)">${body}</g>` : body}</g>`;
  const cells: Array<[number, number]> = [];
  if (c.cells && c.hits.length) {
    const S = [0.1, 0.5, 0.9];
    for (let y = Math.floor(c.bb[1]); y < Math.ceil(c.bb[3]); y++)
      for (let x = Math.floor(c.bb[0]); x < Math.ceil(c.bb[2]); x++) {
        let on = false;
        for (const sy of S) for (const sx of S) if (!on && c.hits.some((h) => h(x + sx, y + sy))) on = true;
        if (on) cells.push([c.flip ? -x - 1 : x, y]);
      }
    cells.sort((a, b) => a[1] - b[1] || a[0] - b[0]);
  }
  return { svg, cells, box: box4(c.vb, c.flip) };
}

// ---- a figura (sem espelho: de frente, olha para sudoeste, em 3/4 para a esquerda da tela)

/* Cabeça de frente: crânio largo, bochecha à esquerda, queixo puxado para a esquerda; orelha à direita. */
const HEAD_F: Node[] = [
  [-0.6, -16.2], [2.9, -15.7], [5.0, -13.7], [5.6, -10.8], [5.0, -8.0], [3.4, -6.2], [1.0, -5.2], [-1.3, -4.95],
  [-3.4, -5.4], [-5.2, -7.0], [-6.0, -9.6], [-5.9, -12.6], [-4.3, -15.3],
];
/* De costas (olhando para noroeste): nuca, e a orelha esquerda à esquerda da tela. */
const HEAD_B: Node[] = [
  [-0.3, -16.3], [3.4, -15.6], [5.6, -13.0], [5.9, -10.2], [4.9, -7.4], [2.4, -5.6], [-0.4, -5.2], [-3.2, -5.6],
  [-5.4, -7.4], [-6.1, -10.2], [-5.7, -13.0], [-3.8, -15.5],
];
const EAR_F: Node[] = [[4.6, -10.7], [5.6, -11.15], [6.45, -10.4], [6.55, -9.2], [6.1, -8.15], [5.0, -7.9]];
const EAR_B: Node[] = EAR_F.map((n) => [-n[0], n[1]] as Node);
/* Olhos: o de longe (à esquerda, mais estreito) e o de perto (à direita, maior). */
interface Eye { x: number; y: number; w: number; h: number; far: boolean }
const EYES: readonly Eye[] = [
  { x: -3.75, y: -9.2, w: 1.85, h: 2.6, far: true },
  { x: 0.65, y: -9.2, w: 2.3, h: 2.8, far: false },
];
const MOUTH: Pt = [-1.75, -6.45];
const TORSO: Node[] = [
  [-0.6, -6.1], [2.4, -5.9], [3.6, -4.9], [3.75, -2.0], [3.55, 0.8, 1], [-3.95, 0.8, 1], [-4.15, -2.0], [-4.1, -4.8], [-3.1, -5.9],
];
const HIPS: Node[] = [[-3.75, -0.5, 1], [3.35, -0.5, 1], [3.45, 1.4], [2.4, 2.2], [-2.9, 2.2], [-3.9, 1.4]];
const LAP: Node[] = [[-4.0, 0.0, 1], [3.6, 0.0, 1], [3.9, 6.2, 1], [-4.3, 6.2, 1]];
const SH_L: Pt = [-3.25, -4.35], SH_R: Pt = [2.85, -4.35];

// ---- cabelo

interface Hair { front?: Node[]; back?: Node[]; bump?: Shape; band?: Shape; shine?: Node[]; tufts?: Node[][] }
/* Contorno de nuvem (cabelo cacheado): picos lisos e vales com quina ao longo de um arco de elipse (graus). */
function scallop(cx: number, cy: number, rx: number, ry: number, a0: number, a1: number, n: number, bump: number): Node[] {
  const out: Node[] = [];
  for (let i = 0; i <= 2 * n; i++) {
    const a = ((a0 + ((a1 - a0) * i) / (2 * n)) * Math.PI) / 180, peak = i % 2 === 1, r = peak ? 1 + bump : 1 - bump * 0.15;
    const x = cx + rx * r * Math.cos(a), y = cy + ry * r * Math.sin(a);
    out.push(peak ? [x, y] : [x, y, 1]);
  }
  return out;
}
/* Cabelo de frente: franja (front), parte de trás que aparece além da cabeça (back), coque (bump), prendedor
   (band), brilho (shine) e mechas sob o gorro (tufts). */
function hairFront(st: string): Hair {
  if (st === "fade") return {
    front: [[-5.85, -12.2, 1], [-6.1, -13.8], [-5.15, -15.85], [-2.6, -16.95], [0.6, -17.05], [3.7, -16.35], [5.6, -14.3], [5.95, -12.0], [5.5, -10.3, 1], [4.75, -11.2], [4.2, -12.5], [3.0, -13.1], [0.6, -13.05], [-2.2, -13.15], [-4.3, -13.0], [-5.2, -12.6]],
    shine: [[-3.4, -16.05], [-1.2, -16.45], [1.2, -16.3]],
  };
  if (st === "curly") return {
    front: [...scallop(-0.2, -12.4, 7.0, 5.6, 158, 385, 6, 0.13), [6.2, -8.6], [5.1, -8.2, 1], [4.6, -10.4], [4.2, -12.1, 1], [3.2, -11.6], [2.4, -12.6, 1], [1.4, -11.7], [0.4, -12.7, 1], [-0.8, -11.8], [-1.9, -12.7, 1], [-3.0, -11.7], [-4.0, -12.4, 1], [-5.0, -11.1], [-5.6, -10.0, 1]],
    shine: [[-3.9, -16.2], [-1.4, -17.2], [1.3, -17.0]],
  };
  if (st === "long") return {
    back: [[-6.7, -12.6], [-7.0, -8.0], [-6.8, -3.8], [-5.8, -0.9, 1], [-3.0, -1.6], [0.2, -2.0], [3.4, -1.6], [6.2, -0.9, 1], [7.0, -4.4], [7.1, -9.4], [6.4, -14.0], [3.6, -16.9], [-0.4, -17.5], [-4.4, -16.5]],
    front: [[-6.2, -10.4, 1], [-6.6, -13.6], [-5.4, -15.9], [-2.6, -17.3], [0.8, -17.5], [4.0, -16.75], [6.05, -14.5], [6.55, -11.2], [6.5, -7.6], [5.8, -4.8, 1], [4.75, -7.8], [4.4, -11.0], [3.4, -12.8], [1.8, -12.1, 1], [0.0, -13.4], [-1.9, -12.5], [-3.6, -11.6, 1], [-4.6, -12.6], [-5.5, -11.4]],
    shine: [[-3.8, -16.1], [-1.2, -16.75], [1.6, -16.6]],
  };
  if (st === "bun" || st === "topknot") return {
    front: [[-6.05, -11.2, 1], [-6.35, -13.5], [-5.3, -15.8], [-2.5, -17.15], [0.8, -17.35], [3.9, -16.65], [5.9, -14.5], [6.25, -11.7], [5.6, -9.6, 1], [4.85, -10.9], [4.2, -12.5], [2.5, -13.3], [0.1, -13.2], [-2.5, -12.6], [-4.6, -11.8]],
    bump: st === "bun" ? ell(3.75, -17.05, 2.15, 2.05) : ell(-0.3, -18.15, 1.75, 1.45),
    band: st === "bun" ? undefined : ell(-0.3, -16.95, 1.0, 0.42),
    shine: [[-3.7, -16.0], [-1.2, -16.6], [1.5, -16.5]],
  };
  if (st === "beanie") return {
    tufts: [
      [[-6.15, -11.3, 1], [-6.45, -12.9], [-5.0, -13.5], [-4.6, -12.4, 1], [-5.3, -12.2]],
      [[4.4, -12.9], [6.05, -13.2], [6.3, -11.2], [5.6, -9.6, 1], [4.9, -11.2]],
    ],
  };
  return {
    front: [[-6.25, -11.2, 1], [-6.55, -13.3], [-5.5, -15.7], [-2.8, -17.15], [0.6, -17.4], [3.8, -16.7], [5.85, -14.6], [6.3, -11.9], [5.65, -9.5, 1], [5.05, -10.6], [4.6, -12.1], [3.2, -12.9], [2.1, -12.3, 1], [1.2, -13.1], [-0.3, -12.2, 1], [-1.4, -13.0], [-3.0, -12.0, 1], [-4.0, -12.75], [-5.2, -11.0, 1]],
    shine: [[-3.8, -16.1], [-1.3, -16.7], [1.4, -16.55]],
  };
}
/* Cabelo de costas: cobre o alto e a nuca até acima da gola (a nuca de pele aparece embaixo, entre as orelhas). */
function hairBack(st: string): Hair {
  if (st === "curly") return { front: [...scallop(-0.2, -12.0, 7.0, 5.8, 150, 390, 7, 0.13), [5.2, -7.0], [2.6, -6.6, 1], [0.0, -7.2], [-2.6, -6.6, 1], [-5.6, -7.2]] };
  if (st === "long") return { front: [[-6.6, -12.6], [-7.0, -8.0], [-6.8, -3.8], [-5.8, -0.9, 1], [-3.0, -1.4], [0.2, -1.8], [3.4, -1.4], [6.2, -0.9, 1], [7.0, -4.4], [7.0, -9.4], [6.3, -14.0], [3.6, -16.9], [-0.4, -17.5], [-4.4, -16.5]] };
  const cap: Node[] = [[-5.05, -9.6, 1], [-6.25, -11.6], [-5.9, -14.4], [-3.8, -16.6], [-0.3, -17.2], [3.4, -16.6], [5.75, -14.4], [6.2, -11.2], [5.3, -8.6], [3.5, -7.85, 1], [1.2, -8.3], [-1.4, -8.25], [-3.6, -7.8, 1], [-4.5, -8.5]];
  if (st === "bun") return { front: cap, bump: ell(1.0, -16.2, 2.3, 2.15) };
  if (st === "topknot") return { front: cap, bump: ell(-0.3, -18.15, 1.75, 1.45), band: ell(-0.3, -16.95, 1.0, 0.42) };
  if (st === "beanie") return { front: [[-5.6, -9.4, 1], [-6.2, -12.4], [6.2, -12.4], [5.6, -8.6], [3.6, -7.8, 1], [0.0, -8.3], [-3.6, -7.8, 1]] };
  return { front: cap };
}
/* Brilho do cabelo: faixa curva perto do topo, do lado da luz (no espelho, do outro lado no espaço local). */
function shine(c: Ctx, nodes: Node[], w: number, color: string): string {
  const s = curve(nodes.map((n) => [c.side < 0 ? n[0] : -n[0] - 0.6, n[1]] as Node));
  return `<path d="${dOf(s)}" fill="none" stroke="${color}" stroke-width="${f(w)}"/>`;
}
const BEANIE: Node[] = [[-6.75, -12.7, 1], [-6.75, -15.0], [-5.1, -17.4], [-1.6, -18.6], [2.0, -18.5], [5.0, -17.0], [6.65, -14.6], [6.75, -12.5, 1], [3.0, -12.85], [-2.0, -13.05]];
const BRIM: Node[] = [[-6.95, -12.5, 1], [-6.95, -14.6, 1], [-2.0, -15.15], [3.0, -15.0], [6.95, -14.4, 1], [6.95, -12.3, 1], [3.0, -12.65], [-2.0, -12.85]];

// ---- tronco e roupa

/* Xadrez do Grêmio: blocos grandes celeste e branco, nas cores da luz ou da sombra. */
function checker(lit: boolean): string {
  let s = `<rect x="-5" y="-7.5" width="11" height="9" fill="${lit ? "#f3f6f9" : "#cfdae6"}"/><path d="`;
  for (let j = 0; j < 4; j++) for (let i = 0; i < 4; i++) if ((i + j) % 2 === 0) s += `M${f(-4.7 + i * 2.35)} ${f(-6.6 + j * 2.35)}h2.35v2.35h-2.35z`;
  return s + `" fill="${lit ? "#a9d3f2" : "#80b0d9"}"/>`;
}
function torso(c: Ctx, p: Pal, l: ChibiLook, view: ChibiView, seated: boolean): void {
  if (seated) part(c, spline(LAP), p.pants, p.pantsSh, { off: [0.9, 0.5] });
  const o = l.outfit, coat = o === "coat";
  const base = coat ? "#eef0f3" : p.shirt, shade = coat ? "#c3c9d4" : p.shirtSh;
  let inner = "";
  if (view === "front") {
    /* Decote (pele) e o que aparece pela frente aberta (blusa do cardigã, camisa sob o jaleco). */
    if (o === "cardigan") inner += `<path d="M-2.05 -6.2L.3 -6.2L.35 1.2L-1.95 1.2Z" fill="#efe8dc"/>`;
    if (coat) inner += `<path d="M-2.4 -6.2L.75 -6.2L.55 1.2L-2.15 1.2Z" fill="${p.shirt}"/>`;
    if (o === "vneck") inner += `<path d="M-2.5 -6.3L.7 -6.3L-.9 -3.5Z" fill="${p.skin}"/>`;
    else if (o === "cardigan" || coat) inner += `<path d="M-2.4 -6.3L.75 -6.3L-.85 -3.9Z" fill="${p.skin}"/>`;
    else inner += `<path d="M-2.6 -6.3Q-.9 -4.4 .85 -6.3Z" fill="${p.skin}"/>`;
    /* Sombra da cabeça logo abaixo do queixo: película escura sobre o que estiver ali. */
    inner += `<path d="${dOf(spline(HEAD_F), -c.side * 0.45, 0.95)}" fill="#2a1424" fill-opacity=".3"/>`;
    if (o === "vneck") inner += `<path d="M-2.95 -6.45L-.9 -3.25L1.15 -6.45" fill="none" stroke="${mix(p.shirt, INK, 0.35)}" stroke-width=".45"/>`;
    if (o === "gremio") inner += `<path d="M-3 -6.35Q-.9 -3.9 1.2 -6.35" fill="none" stroke="#2f7fc4" stroke-width=".7"/>`;
  } else if (o === "gremio") inner += `<path d="M-2.8 -5.95Q-.3 -5.2 2.2 -5.95" fill="none" stroke="#2f7fc4" stroke-width=".7"/>`;
  part(c, spline(TORSO), base, shade, { off: [1.0, 0.35], inner, print: o === "gremio" ? [checker(false), checker(true)] : undefined });
  if (view === "front") outfitFront(c, p, o);
  else outfitBack(c, p, o);
}
/* Detalhes da roupa por cima do tronco, de frente. */
function outfitFront(c: Ctx, p: Pal, o: string): void {
  const thin = LINE * 0.8, loose = true;
  if (o === "hoodie") {
    part(c, spline([[-4.0, -5.2], [-3.1, -6.7], [-0.5, -7.1], [2.5, -6.7], [3.4, -5.3], [2.1, -4.3], [-0.4, -4.0], [-2.9, -4.4]]), p.shirt, p.shirtSh, { off: [0.6, 0.5], loose });
    part(c, spline([[-2.5, -1.9, 1], [1.5, -1.9, 1], [2.4, 0.8, 1], [-3.3, 0.8, 1]]), p.shirt, p.shirtSh, { off: [0.3, 0.35], loose, line: thin });
    stroke(c, polyline([[-1.75, -4.5], [-1.95, -2.7]]), 0.3, WHITE);
    stroke(c, polyline([[0.55, -4.4], [0.7, -2.8]]), 0.3, WHITE);
  } else if (o === "lanyard") {
    stroke(c, polyline([[-2.3, -5.6], [-1.15, -2.6], [0.6, -5.6]]), 0.38, "#e3ba6a");
    part(c, spline([[-2.0, -2.8, 1], [-0.2, -2.8, 1], [-0.2, -0.5, 1], [-2.0, -0.5, 1]]), WHITE, "#c9ccd4", { off: [0.3, 0.3], loose, line: thin, inner: `<rect x="-2.1" y="-2.9" width="2" height=".75" fill="#5b6ad9"/>` });
  } else if (o === "cardigan") {
    stroke(c, polyline([[-2.05, -5.0], [-1.95, 0.8]]), thin);
    stroke(c, polyline([[0.3, -5.0], [0.35, 0.8]]), thin);
    for (const y of [-3.6, -2.0, -0.4]) flat(c, ell(0.75, y, 0.28, 0.28), "#efe8dc", 0.18);
  } else if (o === "pearls") {
    for (let i = 0; i <= 6; i++) {
      const t = i / 6;
      flat(c, ell(-2.9 + t * 4.1, -5.55 + Math.sin(t * Math.PI) * 1.35, 0.3, 0.3), "#fbf7f0", 0.16, "#8b7d70");
    }
  } else if (o === "coat") {
    stroke(c, polyline([[-2.4, -6.0], [-2.15, 0.8]]), thin);
    stroke(c, polyline([[0.75, -6.0], [0.55, 0.8]]), thin);
    stroke(c, polyline([[-2.6, -5.9], [-1.3, -3.7], [-2.25, -2.6]]), thin);
    stroke(c, polyline([[0.95, -5.9], [-0.1, -3.7], [0.65, -2.6]]), thin);
    part(c, spline([[1.4, -3.0, 1], [3.0, -3.0, 1], [3.0, -1.6, 1], [1.4, -1.6, 1]]), "#eef0f3", "#c3c9d4", { off: [0.2, 0.2], loose, line: thin * 0.8 });
    stroke(c, polyline([[2.3, -3.6], [2.3, -2.6]]), 0.36, "#5b6ad9");
  } else if (o === "scarf") {
    part(c, spline([[-4.0, -5.6], [-2.6, -6.9], [-0.4, -7.2], [2.3, -6.9], [3.4, -5.6], [2.2, -4.5], [-0.4, -4.2], [-2.9, -4.5]]), "#e3ba6a", "#b8893a", { off: [0.6, 0.5], loose, inner: `<path d="M-3.4 -6.7L-2.6 -4M-1.4 -7.3L-.8 -4M.8 -7.3L1.2 -4.1M2.6 -7L2.9 -4.6" stroke="#d9645a" stroke-width=".5"/>` });
    part(c, spline([[-2.9, -4.8, 1], [-1.3, -4.8, 1], [-1.2, -0.9, 1], [-2.8, -0.9, 1]]), "#e3ba6a", "#b8893a", { off: [0.3, 0.3], loose, inner: `<path d="M-3 -3.6H-1M-3 -2.2H-1" stroke="#d9645a" stroke-width=".45"/>` });
    for (const x of [-2.6, -2.0, -1.4]) stroke(c, polyline([[x, -0.85], [x, -0.22]]), 0.3, "#b8893a");
  } else if (o === "tie") {
    part(c, pgon([[-2.6, -6.0], [-0.9, -4.9], [-2.2, -4.1]]), WHITE, "#c9ccd4", { off: [0.15, 0.15], loose, line: thin });
    part(c, pgon([[0.8, -6.0], [-0.9, -4.9], [0.4, -4.1]]), WHITE, "#c9ccd4", { off: [0.15, 0.15], loose, line: thin });
    part(c, spline([[-1.45, -5.0, 1], [-0.35, -5.0, 1], [-0.55, -4.25, 1], [-1.25, -4.25, 1]]), "#3f4a63", "#2a3245", { off: [0.1, 0.1], loose, line: thin });
    part(c, spline([[-1.25, -4.3, 1], [-0.55, -4.3, 1], [-0.15, -1.3, 1], [-0.9, -0.55, 1], [-1.65, -1.3, 1]]), "#3f4a63", "#2a3245", { off: [0.25, 0.2], loose, line: thin });
  } else if (o === "gremio") {
    /* Escudo (anel escuro, campo azul, faixas branca e preta) e três estrelas: bronze, prata e ouro. */
    part(c, spline([[0.65, -3.4, 1], [2.45, -3.4, 1], [2.45, -2.4], [1.55, -1.2, 1], [0.65, -2.4]]), "#3f95e0", "#2f7fc4", { off: [0.15, 0.15], loose, line: 0.22, ink: "#15171d", inner: `<path d="M.6 -2.7H2.5" stroke="#f3f6f9" stroke-width=".45"/><path d="M.6 -2.25H2.5" stroke="#15171d" stroke-width=".3"/>` });
    star(c, 0.85, -3.95, 0.36, "#d08c50");
    star(c, 1.55, -4.15, 0.36, "#e8ebf0");
    star(c, 2.25, -3.95, 0.36, "#f2c44c");
  }
}
function star(c: Ctx, x: number, y: number, r: number, col: string): void {
  const pts: Pt[] = [];
  for (let i = 0; i < 10; i++) {
    const a = -Math.PI / 2 + (i * Math.PI) / 5, rr = i % 2 ? r * 0.45 : r;
    pts.push([x + rr * Math.cos(a), y + rr * Math.sin(a)]);
  }
  flat(c, pgon(pts), col, 0.12, "#5a4a2a");
}
/* Detalhes da roupa de costas. */
function outfitBack(c: Ctx, p: Pal, o: string): void {
  const loose = true;
  if (o === "hoodie") part(c, spline([[-3.4, -5.6], [-1.8, -6.6], [0.8, -6.6], [2.6, -5.6], [2.0, -3.0], [-0.4, -2.2], [-2.8, -3.0]]), p.shirt, p.shirtSh, { off: [0.5, 0.5], loose });
  else if (o === "coat") stroke(c, polyline([[-0.3, -2.0], [-0.3, 0.8]]), LINE * 0.8);
  else if (o === "scarf") part(c, spline([[-3.8, -5.4], [-2.4, -6.6], [-0.3, -6.9], [2.0, -6.6], [3.2, -5.4], [2.0, -4.6], [-0.3, -4.4], [-2.6, -4.6]]), "#e3ba6a", "#b8893a", { off: [0.5, 0.5], loose, inner: `<path d="M-2.6 -6.8L-2 -4.2M0 -7.2L.3 -4.2M2.2 -6.8L2.5 -4.5" stroke="#d9645a" stroke-width=".5"/>` });
  else if (o === "lanyard") stroke(c, polyline([[-2.6, -5.3], [-0.3, -6.0], [2.0, -5.3]]), 0.36, "#e3ba6a");
}

// ---- cabeça e rosto

/* Sobrancelha: faixa curva de `a` (ponta de dentro) a `b`, com arco `arch` e espessura de t0 a t1. */
function brow(c: Ctx, a: Pt, b: Pt, arch: number, t0: number, t1: number, color: string): void {
  const mx = (a[0] + b[0]) / 2, my = (a[1] + b[1]) / 2 + arch, top: Node[] = [], bot: Node[] = [];
  const s = b[0] > a[0] ? 1 : -1;
  for (let i = 0; i <= 4; i++) {
    const t = i / 4, u = 1 - t;
    const x = u * u * a[0] + 2 * u * t * mx + t * t * b[0], y = u * u * a[1] + 2 * u * t * my + t * t * b[1];
    const dx = 2 * u * (mx - a[0]) + 2 * t * (b[0] - mx), dy = 2 * u * (my - a[1]) + 2 * t * (b[1] - my), l = Math.hypot(dx, dy) || 1;
    const w = (t0 + (t1 - t0) * t) / 2, nx = -dy / l, ny = dx / l, end = i === 0 || i === 4 ? 1 : 0;
    top.push(end ? [x - nx * w * s, y - ny * w * s, 1] : [x - nx * w * s, y - ny * w * s]);
    bot.push(end ? [x + nx * w * s, y + ny * w * s, 1] : [x + nx * w * s, y + ny * w * s]);
  }
  flat(c, spline([...top, ...bot.reverse()]), color, 0.14, mix(color, INK, 0.5));
}
/* Barba: faixa no maxilar (da costeleta ao queixo, com as bochechas livres), bigode e cavanhaque mais escuros.
   Curta: tons perto da pele e sem traço; cheia: mais escura, com traço fino. */
const JAW: Node[] = [
  [4.9, -10.1, 1], [5.05, -8.0], [3.4, -6.15], [1.0, -5.1], [-1.3, -4.8], [-3.45, -5.3], [-5.2, -6.95], [-5.8, -8.5, 1],
  [-5.0, -7.55], [-4.1, -6.5], [-2.9, -6.05], [-1.6, -6.2], [-0.3, -6.05], [1.3, -6.5], [3.0, -7.35], [4.25, -8.7],
];
function beard(c: Ctx, p: Pal, kind: string): void {
  const full = kind === "full", jaw = spline(JAW), tone = full ? 0.72 : 0.42;
  if (full) part(c, jaw, mix(p.skin, p.beard, tone), mix(p.skinSh, p.beardSh, tone + 0.05), { off: [0.6, 0.45], line: 0.2, ink: mix(p.beard, INK, 0.5), loose: true });
  else flat(c, jaw, mix(p.skin, p.beard, tone));
  const dark = full ? p.beard : mix(p.skin, p.beard, 0.75);
  flat(c, spline([[-3.35, -6.8], [-2.6, -7.45], [-1.7, -7.6], [-0.7, -7.4], [0.0, -6.75], [-0.7, -6.95], [-1.7, -7.05], [-2.7, -6.9]]), dark);
  flat(c, spline([[-2.75, -5.95], [-1.65, -6.12], [-0.55, -5.95], [-0.85, -4.95], [-1.65, -4.72], [-2.45, -4.95]]), dark);
}
function face(c: Ctx, p: Pal): void {
  /* Sobrancelhas grossas, afinando para fora, com a ponta de dentro mais baixa (cara de determinação). */
  brow(c, [-2.85, -11.25], [-5.0, -11.05], -0.35, 0.7, 0.36, p.brow);
  brow(c, [-0.5, -11.3], [1.95, -11.25], -0.45, 0.74, 0.38, p.brow);
  /* Nariz: traço curto apontando para a esquerda. */
  stroke(c, curve([[-2.2, -8.3], [-2.55, -7.75], [-2.1, -7.65]]), 0.26, p.skinDk);
  /* Pálpebras fechadas (aparecem quando a camada dos olhos some: a piscada). */
  for (const e of EYES) stroke(c, curve([[e.x - e.w / 2, e.y + 0.15], [e.x, e.y + 0.65], [e.x + e.w / 2 + 0.1, e.y + 0.1]]), 0.4);
  /* Bochechas coradas. */
  flat(c, ell(-4.55, -7.55, 0.75, 0.38), p.blush);
  flat(c, ell(2.15, -7.5, 0.95, 0.42), p.blush);
}
function accessory(c: Ctx, a: string | null, view: ChibiView): void {
  if (a === "headset") {
    const band = "#3d424d", cup = "#2b2e36";
    if (view === "front") {
      stroke(c, curve([[-5.6, -13.4], [-3.4, -16.9], [1.2, -17.75], [4.8, -15.6], [6.0, -11.6]]), 0.62, band);
      part(c, ell(5.75, -9.6, 1.25, 1.65), band, cup, { off: [0.3, 0.3], loose: true, inner: `<ellipse cx="5.45" cy="-9.75" rx=".55" ry=".8" fill="#e0894e"/>` });
      stroke(c, curve([[5.2, -8.4], [3.6, -6.4], [1.0, -6.0]]), 0.32, band);
      flat(c, ell(0.75, -6.0, 0.45, 0.38), cup, 0.18);
    } else {
      stroke(c, curve([[-6.0, -11.6], [-4.0, -16.4], [0.6, -17.6], [5.0, -15.8], [5.9, -12.6]]), 0.62, band);
      part(c, ell(-5.75, -9.6, 1.25, 1.65), band, cup, { off: [0.3, 0.3], loose: true });
    }
  } else if (a === "goggles") {
    stroke(c, view === "front" ? curve([[-6.3, -12.5], [-1.0, -13.7], [6.2, -12.8]]) : curve([[-6.2, -12.4], [-0.3, -12.2], [6.2, -12.4]]), 0.75, "#5b6ad9");
    if (view === "front") for (const [x, w] of [[-3.6, 1.25], [0.6, 1.5]] as Pt[]) {
      part(c, ell(x, -13.55, w, 1.05), "#a6b0f7", "#7b88e6", { off: [0.3, 0.3], loose: true, line: 0.3, inner: `<path d="M${f(x - w * 0.45)} -13.85l${f(w * 0.35)} -.45" stroke="#f4f5f7" stroke-width=".3"/>` });
    }
  } else if (view === "front" && (a === "glasses" || a === "readers")) glasses(c, a);
}
/* Óculos: aro em volta dos olhos (lente sem cor: a piscada não muda o tom da pele) e haste até a orelha.
   De leitura: meia-lua mais baixa, aro marrom. */
function glasses(c: Ctx, a: string): void {
  const rd = a === "readers", col = rd ? "#7a3b2a" : "#2b2e36", w = rd ? 0.26 : 0.3, g = rd ? 0.35 : 0.42;
  const lens = (e: Eye): Shape => rd
    ? spline([[e.x - e.w / 2 - 0.35, e.y + 0.25, 1], [e.x + e.w / 2 + 0.35, e.y + 0.25, 1], [e.x + e.w / 2 + 0.15, e.y + 1.2], [e.x, e.y + 1.6], [e.x - e.w / 2 - 0.15, e.y + 1.2]])
    : ell(e.x, e.y + 0.05, e.w / 2 + 0.45, e.h / 2 + 0.25);
  const [ef, en] = EYES, y = rd ? en.y + 0.45 : en.y - 0.35;
  stroke(c, lens(ef), w, col);
  stroke(c, lens(en), w, col);
  stroke(c, polyline([[ef.x + ef.w / 2 + g, y], [en.x - en.w / 2 - g, y]]), w, col);
  stroke(c, polyline([[en.x + en.w / 2 + g, y], [4.9, y - 0.4]]), w, col);
  stroke(c, polyline([[en.x - 0.5, en.y - 0.75], [en.x + 0.05, en.y - 1.05]]), 0.2, "#ffffffb0");
}
function ribs(p: Pal): string {
  let d = "";
  for (let x = -6.4; x <= 6.6; x += 0.9) d += `M${f(x)} -15.4V-12.2`;
  return `<path d="${d}" stroke="${mix(p.shirtSh, "#000000", 0.3)}" stroke-width=".22"/>`;
}
function beanie(c: Ctx, p: Pal, pom: number): void {
  part(c, spline(BEANIE), p.shirt, p.shirtSh, { off: [0.85, 0.6] });
  part(c, spline(BRIM), p.shirtSh, mix(p.shirtSh, "#000000", 0.25), { off: [0.4, 0.35], inner: ribs(p) });
  part(c, ell(pom, -18.9, 1.45, 1.35), "#f2efe9", "#c9c4bb", { off: [0.45, 0.4] });
}
const earLine = (x: number): string => `M${f(x * 5.45)} -10.1C${f(x * 6)} -9.9 ${f(x * 6)} -8.9 ${f(x * 5.35)} -8.65`;

function head(c: Ctx, p: Pal, l: ChibiLook, view: ChibiView): void {
  const st = l.hair_style, front = view === "front", hr = front ? hairFront(st) : hairBack(st);
  if (front) {
    part(c, spline(EAR_F), p.skin, p.skinSh, { off: [0.45, 0.4], inner: `<path d="${earLine(1)}" fill="none" stroke="${p.skinDk}" stroke-width=".26"/>` });
    if (hr.back) part(c, spline(hr.back), p.hair, p.hairSh, { off: [0.8, 0.6] });
    /* Sombra da franja (ou do gorro) na testa. */
    const cast = st === "beanie" ? spline(BRIM) : hr.front ? spline(hr.front) : null;
    part(c, spline(HEAD_F), p.skin, p.skinSh, { off: [0.95, 0.55], inner: cast ? `<path d="${dOf(cast, -c.side * 0.3, 0.6)}" fill="${p.skinSh}"/>` : "" });
    if (l.beard === "stubble" || l.beard === "full") beard(c, p, l.beard);
  } else {
    part(c, spline(HEAD_B), p.skin, p.skinSh, { off: [0.9, 0.5] });
    part(c, spline(EAR_B), p.skin, p.skinSh, { off: [0.45, 0.4], inner: `<path d="${earLine(-1)}" fill="none" stroke="${p.skinDk}" stroke-width=".26"/>` });
  }
  if (hr.bump) part(c, hr.bump, p.hair, p.hairSh, { off: [0.6, 0.5] });
  if (hr.tufts) for (const t of hr.tufts) part(c, spline(t), p.hair, p.hairSh, { off: [0.4, 0.4] });
  if (hr.front) {
    const fade = st === "fade";
    /* Degradê: as laterais raspadas num tom entre o cabelo e a pele (de frente, da têmpora até a orelha). */
    let inner = "";
    if (fade) inner += front
      ? `<path d="M2.7 -13L3.5 -14.15C4.4 -14.6 5.6 -14.7 7 -14.4V-9H2.7ZM-4.55 -12.8L-4.95 -13.95C-5.6 -14.35 -6.4 -14.45 -7 -14.3V-11H-4.55Z" fill="${p.fade}"/>`
      : `<path d="M-7 -12.4C-3 -11.6 3 -11.6 7 -12.4V-6H-7Z" fill="${p.fade}"/>`;
    if (st !== "beanie") inner += shine(c, front ? hr.shine || [] : [[-3.6, -15.9], [-0.6, -16.6], [2.4, -16.0]], fade ? 0.38 : 0.5, p.hairLt);
    if (fade) inner += `<path d="M-3.9 -15.3l.5 -.25M-1.9 -15.9l.55 -.2M.4 -15.75l.5 -.1M2.4 -15.2l.45 .05M-.8 -14.4l.5 -.15M1.5 -14.2l.45 0" stroke="${p.hairLt}" stroke-width=".22"/>`;
    /* Fios: traços finos de nanquim seguindo a franja; nos cachos, pequenos arcos. */
    if (front && (st === "short" || st === "long" || st === "bun" || st === "topknot")) inner += `<path d="M-4.6 -13.6Q-3.6 -14.8 -2.4 -15.4M-.9 -13.4Q.1 -14.7 1.6 -15.2M2.9 -13.6Q3.9 -14.4 4.7 -14.4" fill="none" stroke="${p.hairSh}" stroke-width=".24"/>`;
    if (st === "curly") inner += `<path d="${front ? "M-4.6 -14.2a.8 .8 0 1 1 1.2 .7M-1.2 -15.4a.8 .8 0 1 1 1.2 .7M2.2 -14.6a.8 .8 0 1 1 1.2 .7M-2.9 -12.9a.7 .7 0 1 1 1 .6M.9 -13a.7 .7 0 1 1 1 .6" : "M-4.4 -13.4a.8 .8 0 1 1 1.2 .7M-1.2 -15a.8 .8 0 1 1 1.2 .7M2.2 -13.8a.8 .8 0 1 1 1.2 .7M-2.6 -10.6a.8 .8 0 1 1 1.2 .7M1.4 -10.4a.8 .8 0 1 1 1.2 .7"}" fill="none" stroke="${p.hairLt}" stroke-width=".22"/>`;
    part(c, spline(hr.front), p.hair, p.hairSh, { off: front ? [0.7, 0.55] : [0.8, 0.6], inner });
  }
  if (hr.band) part(c, hr.band, p.shirt, p.shirtSh, { off: [0.2, 0.2], loose: true });
  if (st === "beanie") beanie(c, p, front ? 0.5 : 0.2);
  if (front) face(c, p);
  accessory(c, l.accessory, view);
}

// ---- olhos e boca (camadas)

function eyeOpen(c: Ctx, e: Eye): void {
  const white = ell(e.x, e.y, e.w / 2, e.h / 2), cid = clipOf(c, white), id = ref(c, white);
  const ix = e.x - 0.18 - (e.far ? 0.05 : 0), iy = e.y + 0.18, irx = e.w * 0.4, iry = e.h * 0.44, hl = c.side;
  c.out.push(
    `<g clip-path="url(#${cid})">${use(id, ` fill="#fbf8f4"`)}` +
      `<path d="${dOf(ell(ix, iy, irx, iry))}" fill="#7a4a2c"/>` +
      `<path d="${dOf(ell(ix, iy - iry * 0.42, irx, iry * 0.62))}" fill="#3a2016"/>` +
      `<path d="${dOf(ell(ix, iy + 0.05, irx * 0.48, iry * 0.5))}" fill="#140b08"/>` +
      `<path d="${dOf(ell(e.x, e.y - e.h / 2, e.w, 0.5))}" fill="#2a1812" fill-opacity=".38"/></g>`,
  );
  c.out.push(use(id, ` fill="none" stroke="${INK}" stroke-width=".18"`));
  grow(c.vb, white.poly, 0.1);
  /* Pálpebra de cima: traço grosso, com a ponta de fora um pouco levantada. */
  const o = e.far ? -1 : 1;
  stroke(c, curve([[e.x - (e.w / 2 + 0.12) * o, e.y + 0.05], [e.x - 0.1 * o, e.y - e.h / 2 - 0.05], [e.x + (e.w / 2 + 0.05) * o, e.y - 0.4], [e.x + (e.w / 2 + 0.22) * o, e.y - 0.5]]), 0.5);
  stroke(c, curve([[e.x + 0.05 * o, e.y + e.h / 2 + 0.12], [e.x + (e.w / 2 - 0.05) * o, e.y + e.h / 2 - 0.3]]), 0.2, "#3a242080");
  flat(c, ell(ix + hl * irx * 0.42, iy - iry * 0.42, 0.36, 0.38), "#ffffff");
  flat(c, ell(ix - hl * irx * 0.35, iy + iry * 0.45, 0.17, 0.17), "#ffffffd0");
}
function eyeHappy(c: Ctx, p: Pal, e: Eye): void {
  stroke(c, curve([[e.x - e.w / 2, e.y + 0.15], [e.x, e.y + 0.65], [e.x + e.w / 2 + 0.1, e.y + 0.1]]), 0.75, p.skin);
  stroke(c, curve([[e.x - e.w / 2 - 0.05, e.y + 0.55], [e.x, e.y - 0.5], [e.x + e.w / 2 + 0.05, e.y + 0.55]]), 0.46);
}
function mouth(c: Ctx, kind: ChibiMouth): void {
  const [mx, my] = MOUTH;
  if (kind === "neutral") stroke(c, curve([[mx - 0.75, my - 0.05], [mx, my + 0.05], [mx + 0.7, my - 0.08]]), 0.3);
  else if (kind === "smile") {
    const s: Shape = { cmds: [["M", mx - 0.95, my - 0.3], ["C", mx - 0.35, my + 0.45, mx + 0.25, my + 0.45, mx + 0.85, my - 0.35]], poly: [[mx - 0.95, my - 0.3], [mx, my + 0.3], [mx + 0.85, my - 0.35]] };
    stroke(c, s, 0.32);
  } else if (kind === "ohh") flat(c, ell(mx, my + 0.05, 0.45, 0.55), "#5a1f1e", 0.26);
  else if (kind === "grin") {
    part(c, spline([[mx - 1.15, my - 0.55, 1], [mx + 1.05, my - 0.6, 1], [mx + 0.55, my + 0.55], [mx - 0.1, my + 0.8], [mx - 0.75, my + 0.5]]), "#6b2522", "#6b2522", {
      off: [0, 0], line: 0.28, loose: true,
      inner: `<rect x="${f(mx - 1.3)}" y="${f(my - 0.7)}" width="2.5" height=".55" fill="#fbf8f1"/><path d="${dOf(ell(mx - 0.1, my + 0.75, 0.65, 0.35))}" fill="#d9645a"/>`,
    });
  } else {
    part(c, spline([[mx - 1.2, my - 0.45, 1], [mx + 1.1, my - 0.6, 1], [mx + 1.0, my + 0.4, 1], [mx - 1.1, my + 0.45, 1]]), "#fbf8f1", "#d6d2c8", {
      off: [0, 0], line: 0.28, loose: true,
      inner: `<path d="M${f(mx - 1.2)} ${f(my)}L${f(mx + 1.05)} ${f(my - 0.08)}M${f(mx - 0.4)} ${f(my - 0.5)}V${f(my + 0.45)}M${f(mx + 0.35)} ${f(my - 0.55)}V${f(my + 0.42)}" stroke="#9a948a" stroke-width=".16"/>`,
    });
  }
}

// ---- braços

/** Pontos dos braços por pose (ombro, cotovelo, mão), em px lógicos relativos ao quadril, sem espelho.
    L e R são os lados da tela, como no POSE do scene.ts: L é o braço de longe, R o de perto. */
export const CHIBI_POSE: Readonly<Record<ChibiPose, ReadonlyArray<readonly Pt[]>>> = {
  hang: [[SH_L, [-4.25, -2.3], [-4.45, -0.35]], [SH_R, [3.85, -2.3], [4.05, -0.35]]],
  restL: [[SH_L, [-5.2, -2.0], [-5.6, 0.45]]],
  restR: [[SH_R, [4.15, -1.5], [-1.8, 2.05]]],
  crossL: [[SH_L, [-4.45, -2.3], [1.85, -2.05]]],
  crossR: [[SH_R, [4.0, -2.5], [-2.45, -2.65]]],
  sipR: [[SH_R, [3.95, -2.2], [-1.15, -5.35]]],
  palmL: [[SH_L, [-6.3, -6.2], [-3.7, -11.75]]],
  stretch0: [[SH_L, [-5.9, -7.2], [-6.5, -10.6]], [SH_R, [5.4, -7.2], [6.1, -10.6]]],
  stretch1: [[SH_L, [-6.4, -7.8], [-7.3, -11.4]], [SH_R, [5.9, -7.8], [6.9, -11.4]]],
  wave0: [[SH_R, [5.7, -6.1], [6.25, -9.6]]],
  wave1: [[SH_R, [5.95, -5.8], [7.35, -8.85]]],
  fold: [[SH_L, [-4.45, -2.6], [-3.0, -1.55]], [SH_R, [4.0, -2.6], [2.25, -1.75]]],
};
/* Mangas por roupa: curta (antebraço de pele) ou comprida; o jaleco tem manga branca. */
const SHORT_SLEEVE = new Set(["lanyard", "gremio", "pearls"]);
function arm(c: Ctx, p: Pal, l: ChibiLook, pts: readonly Pt[], open: boolean): void {
  const [sh, el, hd] = pts, o = l.outfit, coat = o === "coat", short = SHORT_SLEEVE.has(o);
  const sleeve = coat ? "#eef0f3" : o === "gremio" ? "#a9d3f2" : p.shirt;
  const sleeveSh = coat ? "#c3c9d4" : o === "gremio" ? "#80b0d9" : p.shirtSh;
  /* O punho fica um pouco antes do ponto da mão. */
  const fx = hd[0] - el[0], fy = hd[1] - el[1], fl = Math.hypot(fx, fy) || 1, ux = fx / fl, uy = fy / fl;
  const wrist: Pt = [hd[0] - ux * 0.55, hd[1] - uy * 0.55];
  if (short) {
    const cut: Pt = [sh[0] + (el[0] - sh[0]) * 0.8, sh[1] + (el[1] - sh[1]) * 0.8];
    limbs(c, [{ pts: [cut, el, wrist], w: 1.6, base: p.skin, shade: p.skinSh }, { pts: [sh, cut], w: 2.15, base: sleeve, shade: sleeveSh }]);
  } else limbs(c, [{ pts: [sh, el, wrist], w: 1.95, base: sleeve, shade: sleeveSh }]);
  /* Mão: luva redonda; aberta (aceno, alongar, testa) é mais comprida, com os dedos marcados. */
  if (open) {
    part(c, ell(hd[0], hd[1], 1.2, 0.98, (Math.atan2(fy, fx) * 180) / Math.PI), p.skin, p.skinSh, { off: [0.3, 0.3] });
    for (const k of [-0.35, 0.3]) stroke(c, polyline([[hd[0] + ux * 0.35 - uy * k, hd[1] + uy * 0.35 + ux * k], [hd[0] + ux * 1.05 - uy * k, hd[1] + uy * 1.05 + ux * k]]), 0.16, p.skinDk);
  } else part(c, ell(hd[0], hd[1], 1.0, 0.95), p.skin, p.skinSh, { off: [0.3, 0.3] });
  if (!short) stroke(c, polyline([[wrist[0] - uy * 0.95, wrist[1] + ux * 0.95], [wrist[0] + uy * 0.95, wrist[1] - ux * 0.95]]), 0.22, sleeveSh);
}
/* Caneca na boca (sipR): branca, com a alça para o lado da mão. */
function mug(c: Ctx): void {
  const handle = curve([[-2.05, -7.2], [-1.0, -7.0], [-1.25, -6.0], [-2.0, -5.9]]);
  stroke(c, handle, 0.75);
  stroke(c, handle, 0.35, "#e9eaee");
  part(c, spline([[-4.0, -7.6, 1], [-2.0, -7.85, 1], [-1.9, -5.75], [-2.3, -5.3, 1], [-3.7, -5.15, 1], [-4.05, -5.6]]), "#f4f5f7", "#b4b8c0", { off: [0.45, 0.2], inner: `<path d="M-3.95 -7.3L-2 -7.55" stroke="#8b909a" stroke-width=".3"/>` });
}
/* Pasta amarela carregada na frente (fold), com uma folha aparecendo. */
function folder(c: Ctx): void {
  part(c, spline([[-3.45, -3.9, 1], [-1.2, -3.9, 1], [-0.8, -3.45, 1], [2.55, -3.45, 1], [2.55, -0.2, 1], [-3.45, -0.2, 1]]), "#e3ba6a", "#9c7226", { off: [0.3, 0.3] });
  part(c, spline([[-2.6, -4.05, 1], [1.8, -4.05, 1], [1.8, -2.0, 1], [-2.6, -2.0, 1]]), "#f4f5f7", "#c9ccd4", { off: [0.2, 0.2], inner: `<path d="M-2 -3.4H1.1M-2 -2.75H.4" stroke="#8b909a" stroke-width=".22"/>` });
  part(c, spline([[-3.45, -2.75, 1], [2.55, -2.75, 1], [2.55, -0.2, 1], [-3.45, -0.2, 1]]), "#d9a441", "#9c7226", { off: [0.3, 0.35] });
}

// ---- pernas

const SHOE: Node[] = [[-2.25, -0.25, 1], [-2.3, -1.0], [-1.55, -1.75], [-0.2, -2.0], [1.05, -2.1], [1.45, -1.3], [1.4, -0.1, 1], [0.65, 0.12], [-1.75, 0.1]];
const SHOE_B: Node[] = SHOE.map((n) => (n[2] ? [-n[0] * 0.92, n[1], 1] : [-n[0] * 0.92, n[1]]) as Node);
/* Deslocamento dos pés por quadro: passo à frente (para baixo e para a esquerda) ou pé no ar. */
const STEPS: Record<string, [Pt, Pt]> = {
  stand: [[0, 0], [0, 0]],
  0: [[-0.45, 0.15], [0.45, -0.2]],
  1: [[0, 0], [0.15, -0.95]],
  2: [[0.45, -0.2], [-0.45, 0.15]],
  3: [[0.15, -0.95], [0, 0]],
};
function shoe(c: Ctx, x: number, y: number, back: boolean): void {
  const k = back ? 1 : -1;
  part(c, moved(spline(back ? SHOE_B : SHOE), x, y), WHITE, "#c4c7d0", {
    off: [0.55, 0.5],
    inner: `<rect x="${f(x - 2.4)}" y="${f(y - 0.55)}" width="4" height="1" fill="#d5d7de"/><path d="M${f(x + 0.9 * k)} ${f(y - 1.3)}L${f(x + 0.1 * k)} ${f(y - 0.85)}" stroke="#9aa0ab" stroke-width=".26"/>`,
  });
}

// ---- peças públicas

/** Pernas: quadril, pernas e tênis, num quadro da caminhada (0..3) ou parado. De costas, os tênis mostram o calcanhar. */
export function chibiLegs(look: ChibiLook, step: ChibiStep = "stand", view: ChibiView = "front", opts?: ChibiOpts): ChibiPiece {
  const c = ctx(opts), p = palette(look), d = STEPS[String(step)] || STEPS.stand, back = view === "back";
  const feet: Pt[] = [[-1.85 + d[0][0], 6.0 + d[0][1]], [1.45 + d[1][0], 6.0 + d[1][1]]];
  part(c, spline(HIPS), p.pants, p.pantsSh, { off: [0.8, 0.45] });
  limbs(c, [{ pts: [[-1.75, 1.2], [feet[0][0] + 0.15, feet[0][1] - 1.35]], w: 2.5, base: p.pants, shade: p.pantsSh }]);
  shoe(c, feet[0][0], feet[0][1], back);
  limbs(c, [{ pts: [[1.3, 1.2], [feet[1][0] + 0.05, feet[1][1] - 1.35]], w: 2.5, base: p.pants, shade: p.pantsSh }]);
  shoe(c, feet[1][0], feet[1][1], back);
  return finish(c);
}
/** Sombra no chão, embaixo dos pés (translúcida; fora da máscara de clique). */
export function chibiShadow(opts?: ChibiOpts): ChibiPiece {
  const c = ctx(opts);
  flat(c, ell(-0.2, 6.05, 4.7, 1.2), "#05060a57");
  flat(c, ell(-0.2, 6.05, 3.0, 0.75), "#05060a4d");
  return finish(c);
}
/** Corpo: tronco e cabeça, com as pálpebras fechadas (sem olhos abertos nem boca). seated: o colo desce até +6,2. */
export function chibiBody(look: ChibiLook, view: ChibiView = "front", opts?: ChibiOpts & { seated?: boolean }): ChibiPiece {
  const c = ctx(opts), p = palette(look);
  torso(c, p, look, view, !!(opts && opts.seated));
  head(c, p, look, view);
  return finish(c);
}
/** Olhos abertos (íris e brilho) ou felizes (^ ^). Esconder a camada aberta mostra as pálpebras do corpo: a piscada. */
export function chibiEyes(look: ChibiLook, kind: ChibiEyes = "open", opts?: ChibiOpts): ChibiPiece {
  const c = ctx(opts), p = palette(look);
  for (const e of EYES) if (kind === "happy") eyeHappy(c, p, e); else eyeOpen(c, e);
  if (look.accessory === "glasses" || look.accessory === "readers") glasses(c, look.accessory);
  return finish(c);
}
/** Boca por expressão: neutra, sorriso, sorriso com dentes, careta (dentes cerrados) ou "ohh". O visual fica na
    assinatura como nas outras peças, mas a boca não depende dele. */
export function chibiMouth(_look: ChibiLook, kind: ChibiMouth = "smile", opts?: ChibiOpts): ChibiPiece {
  const c = ctx(opts);
  mouth(c, kind);
  return finish(c);
}
/** Braços de uma pose, com as mãos; na sipR, a caneca; na fold, a pasta amarela na frente. */
export function chibiArms(look: ChibiLook, pose: ChibiPose, opts?: ChibiOpts): ChibiPiece {
  const c = ctx(opts), p = palette(look);
  const open = pose === "palmL" || pose.startsWith("stretch") || pose.startsWith("wave");
  if (pose === "fold") folder(c);
  for (const a of CHIBI_POSE[pose]) arm(c, p, look, a, open);
  if (pose === "sipR") mug(c);
  return finish(c);
}
/** Junta as células de várias peças (máscara de clique do conjunto). */
export function chibiCells(pieces: ChibiPiece[]): Array<[number, number]> {
  const seen = new Set<string>(), out: Array<[number, number]> = [];
  for (const pc of pieces) for (const [x, y] of pc.cells) {
    const k = x + "," + y;
    if (!seen.has(k)) { seen.add(k); out.push([x, y]); }
  }
  return out.sort((a, b) => a[1] - b[1] || a[0] - b[0]);
}
