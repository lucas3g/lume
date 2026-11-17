// @ts-nocheck
/* Cena do escritório isométrico da esteira de bots, vinda do protótipo no Claude Design (v2).
   Grade lógica 360x186 px escalada por inteiro (--s), desenhada por dentro com 2x2 pixels finos por px lógico
   (contorno, luz de borda, sombras e poças de luz). Sprites são mapas de caracteres ampliados pelo Scale2x e
   convertidos em um bitmap (PNG) desenhado como <image> sem suavizar; a animação fica toda no CSS (steps()) de office.css.
   Funções puras: estado da cena -> modelo de desenho. Os tipos públicos ficam no fim do arquivo. */
const LW = 360, LH = 186, HW = 12, HH = 6, OX = 180, OY = 26, GRID = 13, WALL = 24, SPEED = 3.2, HAND = 0.6;
const ORDER = ['triage', 'planner', 'approval', 'implementer', 'reviewer', 'tester', 'code_review', 'reporter'];
/* Estações: área de 2x2 tiles a partir de `at`. Na moldura local, u corre ao longo da mesa (0..2) e v vai da
   cadeira (0..1, atrás da mesa) até a frente da mesa (1..2). Em 'sw' a pessoa olha para sudoeste (u = x, v = y);
   em 'se' olha para sudeste (u = y, v = x), que é o espelho. As de parede ficam de costas para ela. */
const STATIONS = {
  triage: { at: [0, 8], face: 'se' },
  planner: { at: [0, 4], face: 'se' },
  approval: { at: [4, 0], face: 'sw' },
  implementer: { at: [4, 4], face: 'sw' },
  reviewer: { at: [8, 4], face: 'sw' },
  tester: { at: [4, 8], face: 'se' },
  reporter: { at: [8, 8], face: 'sw' },
  /* Na frente do andar, à esquerda: corredor livre dos dois lados e na frente da mesa. */
  code_review: { at: [4, 11], face: 'sw' }
};

/* Projeção isométrica 2:1. x cresce para sudeste, y para sudoeste, z é a altura em px. */
const sx = (tx, ty) => OX + (tx - ty) * HW;
const sy = (tx, ty, z) => OY + (tx + ty) * HH - (z || 0);
/* Profundidade de um ponto do chão (x + y): define o z-index de quem anda e o de cada móvel. */
const depth = p => Math.round((p[0] + p[1]) * 10);
const MEMO = {};
function memo(key, make) { return MEMO[key] || (MEMO[key] = make()); }

/* Geometria de uma estação: moldura local -> tiles, âncora da pessoa (quadril, sobre o assento), caixa na tela,
   rótulo (embaixo da frente da mesa), saídas (de pé atrás da mesa -> corredor) e o tile de entrega, na ponta
   da mesa ao lado da cadeira (quem entrega não cobre o rótulo). */
function geo(role) {
  return memo('geo:' + role, () => {
    const s = STATIONS[role], fx = s.at[0], fy = s.at[1], se = s.face === 'se';
    const w = (u, v) => (se ? [fx + v, fy + u] : [fx + u, fy + v]);
    const pt = (u, v, z) => { const p = w(u, v); return [Math.round(sx(p[0], p[1])), Math.round(sy(p[0], p[1], z))]; };
    const x0 = sx(fx, fy), y0 = sy(fx, fy);
    return {
      fx: fx, fy: fy, se: se, w: w, pt: pt, seat: pt(1, 0.55, 7), tag: pt(1, 2, 0),
      z: (fx + fy + 2) * 10 + 5, box: [x0 - 25, y0 - 30, 50, 56],
      exits: [{ stand: w(0.5, 0.5), out: w(-0.5, 0.5) }, { stand: w(1.5, 0.5), out: w(2.5, 0.5) }],
      front: w(2.5, 1.5), facing: se ? 'ne' : 'nw'
    };
  });
}

/* Paleta: uma letra por cor nos mapas de pixels. S, s, H, h, C, c e P vêm da pessoa (PEOPLE). */
const PAL = { K: '#101114', G: '#d8dbe2', g: '#8b909a', W: '#ecedef', w: '#b4b8c0', e: '#1a1b20', k: '#3a3e48', A: '#d9a441', a: '#e3ba6a', q: '#9c7226', N: '#58b88a', n: '#86d3a6', R: '#d9645a', r: '#efa49b', B: '#5b6ad9', b: '#a6b0f7', P: '#d8dbe2' };
const WOOD = ['#8f7050', '#6f553c', '#5a4431'], WOOD_D = ['#6f553c', '#5a4330', '#48362a'], WOODS = new Set([WOOD[0], WOOD_D[0]]);
const STEEL = ['#3d424d', '#2b2e36', '#1f2127'], LIGHT = ['#d8dbe2', '#b4b8c0', '#8b909a'], AMBER = ['#e3ba6a', '#d9a441', '#9c7226'];
const CASE = ['#3d424d', '#2b2e36', '#1f2127'], SCREEN = '#12151d';

/* Pintura: grade esparsa de pixels -> bitmap (toImage; a ordem de pintura se resolve na grade).
   A grade interna tem R pixels finos por px lógico: as coordenadas da cena seguem em px lógicos, mas faces,
   bordas e contornos saem com mais definição, e os mapas de caracteres passam pelo Scale2x (uma vez em R = 2,
   duas em R = 4). R depende da densidade da tela (ver density): 4 onde há pixels de tela para isso (retina, escala
   alta), 2 nas telas comuns, onde a grade mais fina só seria jogada fora ao reduzir.
   Os desenhos à mão (pessoas, texturas) seguem na grade de 2 por px lógico: em R = 4 as pessoas são ampliadas
   pelo Scale2x e as texturas recebem as coordenadas nessa grade, então o pontilhado não muda de tamanho. */
let R = 2;
/* Pixel fino -> grade de 2 por px lógico (a dos desenhos à mão). */
const half = v => Math.floor(v * 2 / R);
function paint(g, x, y, w, h, c) {
  for (let j = y; j < y + h; j++) { const row = g[j] || (g[j] = {}); for (let i = x; i < x + w; i++) row[i] = c; }
}
/* Cores: '#rrggbb' ou '#rrggbbaa'; mistura com outra cor mantendo o alfa. */
const rgb = c => [1, 3, 5].map(i => parseInt(c.slice(i, i + 2), 16));
function mix(c, d, t) {
  const a = rgb(c), b = rgb(d);
  return '#' + a.map((v, i) => Math.round(v + (b[i] - v) * t).toString(16).padStart(2, '0')).join('') + c.slice(7);
}
const lit = (c, t) => mix(c, '#fff4dc', t), shade = (c, t) => mix(c, '#07080c', t);
/* Scale2x (EPX): cada caractere vira 2x2 e as diagonais ganham degraus finos em vez de blocos. */
function scale2x(src) {
  const h = src.length, w = src[0].length, out = [];
  const at = (i, j) => (j < 0 || j >= h || i < 0 || i >= w ? '.' : src[j][i]);
  for (let j = 0; j < h; j++) {
    const r0 = [], r1 = [];
    for (let i = 0; i < w; i++) {
      const P = src[j][i], A = at(i, j - 1), B = at(i + 1, j), C = at(i - 1, j), D = at(i, j + 1);
      r0.push(C === A && C !== D && A !== B ? A : P, A === B && A !== C && B !== D ? B : P);
      r1.push(D === C && D !== B && C !== A ? C : P, B === D && B !== A && D !== C ? D : P);
    }
    out.push(r0, r1);
  }
  return out;
}
function stamp(g, x, y, lines, pal, flip) {
  const w = Math.max.apply(null, lines.map(l => l.length));
  const src = lines.map(ln => { const a = []; for (let i = 0; i < w; i++) a.push((flip ? ln[ln.length - 1 - i] : ln[i]) || '.'); return a; });
  const up = R === 4 ? scale2x(scale2x(src)) : scale2x(src), x0 = Math.round(x * R), y0 = Math.round(y * R);
  for (let j = 0; j < up.length; j++) for (let i = 0; i < up[j].length; i++) {
    const ch = up[j][i]; if (ch === '.') continue;
    const c = (pal && pal[ch]) || PAL[ch]; if (c) paint(g, x0 + i, y0 + j, 1, 1, c);
  }
}
/* Contorno seletivo: cada pixel vazio encostado no desenho ganha a cor vizinha bem escurecida (sombras e
   brilhos translúcidos não contam). Separa os objetos uns dos outros, como na pixel art desenhada à mão. */
/* A espessura fica em meio px lógico em qualquer R: em R = 4 o contorno cresce mais uma volta com a mesma cor. */
function outline(g) {
  let front = [];
  Object.keys(g).forEach(ys => { const y = +ys, row = g[y]; Object.keys(row).forEach(xs => { const c = row[xs]; if (c.length <= 7) front.push([+xs, y, shade(c, 0.72)]); }); });
  for (let pass = 0; pass < R / 2; pass++) {
    const add = [];
    front.forEach(f => [[1, 0], [-1, 0], [0, 1], [0, -1]].forEach(d => { const nx = f[0] + d[0], ny = f[1] + d[1]; if (!(g[ny] && g[ny][nx])) add.push([nx, ny, f[2]]); }));
    front = [];
    add.forEach(a => { if (!(g[a[1]] && g[a[1]][a[0]])) { paint(g, a[0], a[1], 1, 1, a[2]); front.push(a); } });
  }
}
/* Polígono convexo (pontos em px lógicos) amostrado no centro do pixel fino: arestas 2:1 saem em degraus de 2 px finos. */
function poly(g, pts, c) {
  pts = pts.map(p => [p[0] * R, p[1] * R]);
  let x0 = 1e9, x1 = -1e9, y0 = 1e9, y1 = -1e9;
  pts.forEach(p => { x0 = Math.min(x0, p[0]); x1 = Math.max(x1, p[0]); y0 = Math.min(y0, p[1]); y1 = Math.max(y1, p[1]); });
  for (let y = Math.floor(y0); y < Math.ceil(y1); y++) for (let x = Math.floor(x0); x < Math.ceil(x1); x++) {
    let pos = 0, neg = 0;
    for (let i = 0; i < pts.length; i++) {
      const a = pts[i], b = pts[(i + 1) % pts.length];
      const cr = (b[0] - a[0]) * (y + 0.5 - a[1]) - (b[1] - a[1]) * (x + 0.5 - a[0]);
      if (cr > 1e-9) pos++; else if (cr < -1e-9) neg++;
    }
    if (!pos || !neg) paint(g, x, y, 1, 1, typeof c === 'function' ? c(x, y) : c);
  }
}
/* Pontilhado ordenado (Bayer 4x4) na grade fina: degradês viram degraus de cor alternados, como na pixel art à mão. */
const BAYER = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];
const bayer = (X, Y) => (BAYER[(Y & 3) * 4 + (X & 3)] + 0.5) / 16;
/* Nível t (0..1) em `steps` degraus com pontilhado entre eles. */
const dith = (t, steps, X, Y) => { const q = Math.max(0, t) * steps, n = Math.floor(q); return Math.min(steps, n + (q - n > bayer(X, Y) ? 1 : 0)) / steps; };
/* Ruído fixo por pixel fino (salpicos, desgaste). */
const hash = (X, Y) => { let h = (X * 374761393 + Y * 668265263) | 0; h = Math.imul(h ^ (h >>> 13), 1274126177); return ((h ^ (h >>> 16)) >>> 0) / 4294967296; };
const fr = v => v - Math.floor(v);
/* Cor com sombra/luz em passos (memoizada: as texturas chamam por pixel). */
const TONE = {};
const tone = (c, t) => { if (!t) return c; const k = c + t; return TONE[k] || (TONE[k] = t > 0 ? lit(c, t) : shade(c, -t)); };
/* Recorta um polígono convexo (em tiles) ao quadrado do piso, aresta por aresta (Sutherland-Hodgman). */
function clipFloor(pts) {
  [[0, 0, 1], [0, GRID, -1], [1, 0, 1], [1, GRID, -1]].forEach(e => {
    const ax = e[0], lim = e[1], sg = e[2], inside = q => (q[ax] - lim) * sg >= 0, out = [];
    pts.forEach((q, i) => {
      const p = pts[(i + pts.length - 1) % pts.length], qi = inside(q), pi = inside(p);
      if (qi !== pi) { const t = (lim - p[ax]) / (q[ax] - p[ax]); out.push([p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t]); }
      if (qi) out.push(q);
    });
    pts = out;
  });
  return pts;
}
/* Caixa de ferramentas isométrica sobre uma grade: faces, caixas e carimbos. Cores de caixa: [topo, face SW, face SE]. */
function iso(g) {
  const quad = (p, c) => poly(g, p, c);
  /* A cor pode ser uma textura: (a, b, X, Y) -> cor, com (a, b) as coordenadas do plano no centro do pixel fino
     (X, Y): no tampo (tx, ty); na face Y (tx, z); na face X (ty, z). */
  const L = v => (v + 0.5) / R;
  const top = (x0, y0, x1, y1, z, c) => quad([[sx(x0, y0), sy(x0, y0, z)], [sx(x1, y0), sy(x1, y0, z)], [sx(x1, y1), sy(x1, y1, z)], [sx(x0, y1), sy(x0, y1, z)]],
    typeof c === 'function' ? (X, Y) => { const a = (L(X) - OX) / HW, b = (L(Y) + z - OY) / HH; return c((a + b) / 2, (b - a) / 2, half(X), half(Y)); } : c);
  const faceY = (y, x0, x1, z0, z1, c) => quad([[sx(x0, y), sy(x0, y, z1)], [sx(x1, y), sy(x1, y, z1)], [sx(x1, y), sy(x1, y, z0)], [sx(x0, y), sy(x0, y, z0)]],
    typeof c === 'function' ? (X, Y) => { const tx = (L(X) - OX) / HW + y; return c(tx, OY + (tx + y) * HH - L(Y), half(X), half(Y)); } : c);
  const faceX = (x, y0, y1, z0, z1, c) => quad([[sx(x, y1), sy(x, y1, z1)], [sx(x, y0), sy(x, y0, z1)], [sx(x, y0), sy(x, y0, z0)], [sx(x, y1), sy(x, y1, z0)]],
    typeof c === 'function' ? (X, Y) => { const ty = x - (L(X) - OX) / HW; return c(ty, OY + (x + ty) * HH - L(Y), half(X), half(Y)); } : c);
  /* Luz de borda: a fileira de pixels finos do tampo junto às arestas da frente fica mais clara, e a quina
     vertical entre as duas faces ganha um fio de luz. Só repinta pixels que ainda têm a cor da face. */
  /* has(cor): a cor pertence à face (a base e os tons da textura dela). */
  const rim = (a, b, has, to) => {
    const ax = a[0] * R, ay = a[1] * R, bx = b[0] * R, by = b[1] * R;
    for (let x = Math.floor(Math.min(ax, bx)); x < Math.ceil(Math.max(ax, bx)); x++) {
      const t = (x + 0.5 - ax) / (bx - ax || 1), ye = ay + (by - ay) * t, y = Math.ceil(ye - 0.5) - 1, row = g[y];
      if (row && row[x] && has(row[x])) row[x] = to;
    }
  };
  /* Texturas de caixa opaca: as faces verticais escurecem perto do chão (oclusão, em degraus pontilhados) e a
     madeira ganha veio ao longo do lado mais comprido. Cada textura guarda os tons que pintou, para a luz de borda. */
  const toned = (base, fn) => { const set = new Set([base]); return { set: set, fn: (a, b, X, Y) => { const o = fn(a, b, X, Y); set.add(o); return o; } }; };
  const box = (x0, y0, x1, y1, z0, z1, c) => {
    /* Sombra de contato no chão, puxada para a frente (a luz vem do fundo da sala). */
    if (z0 === 0 && x1 - x0 >= 0.3 && y1 - y0 >= 0.3) top(x0 + 0.04, y0 + 0.04, x1 + 0.14, y1 + 0.14, 0, '#05060a5c');
    if (c[0].length > 7) { faceY(y1, x0, x1, z0, z1, c[1]); faceX(x1, y0, y1, z0, z1, c[2]); top(x0, y0, x1, y1, z1, c[0]); return; }
    const tall = z1 - z0 >= 4, ao = z0 === 0 ? 0.2 : 0.1;
    const side = base => toned(base, (a, z, X, Y) => (tall ? tone(base, -ao * dith(1 - (z - z0) / (z1 - z0), 3, X, Y)) : base));
    const wood = WOODS.has(c[0]), long = x1 - x0 >= y1 - y0, seed = (x0 * 7.3 + y0 * 3.1) % 5;
    const sY = side(c[1]), sX = side(c[2]);
    const tp = toned(c[0], (tx, ty, X, Y) => {
      if (!wood) return c[0];
      const al = long ? tx : ty, pp = long ? ty : tx, n = pp * 9 + 0.45 * Math.sin(al * 4.2 + pp * 11 + seed);
      return fr(n) < 0.16 ? tone(c[0], -0.13) : fr(n) > 0.86 && hash(X, Y) < 0.5 ? tone(c[0], 0.06) : c[0];
    });
    faceY(y1, x0, x1, z0, z1, sY.fn); faceX(x1, y0, y1, z0, z1, sX.fn); top(x0, y0, x1, y1, z1, tp.fn);
    const P = [sx(x0, y1), sy(x0, y1, z1)], Q = [sx(x1, y1), sy(x1, y1, z1)], T = [sx(x1, y0), sy(x1, y0, z1)], hi = lit(c[0], 0.38);
    const onTop = col => tp.set.has(col);
    rim(P, Q, onTop, hi); rim(Q, T, onTop, hi);
    const qx = Math.round(Q[0] * R) - 1;
    for (let y = Math.round(Q[1] * R); y < Math.round(sy(x1, y1, z0) * R); y++) if (g[y] && sY.set.has(g[y][qx])) g[y][qx] = lit(g[y][qx], 0.16);
  };
  /* Poça de luz num plano horizontal: anéis de uma elipse isométrica, mais opacos no centro (degraus, sem degradê). */
  const glow = (tx, ty, z, r, c, a) => {
    const n = 4;
    for (let k = n; k >= 1; k--) {
      const rr = r * k / n, alpha = Math.round(a * (n - k + 1) / n * 255).toString(16).padStart(2, '0');
      let pts = [];
      for (let i = 0; i < 24; i++) { const th = i / 24 * Math.PI * 2; pts.push([tx + rr * Math.cos(th), ty + rr * Math.sin(th)]); }
      /* No chão, a luz não passa das paredes nem da borda do piso. */
      if (!z) pts = clipFloor(pts);
      if (pts.length > 2) poly(g, pts.map(q => [sx(q[0], q[1]), sy(q[0], q[1], z)]), c.slice(0, 7) + alpha);
    }
  };
  /* Carimbo ancorado num ponto do chão: (tx, ty, z) vira o centro da base do mapa. */
  const at = (tx, ty, z, lines, pal, flip) => stamp(g, Math.round(sx(tx, ty) - lines[0].length / 2), Math.round(sy(tx, ty, z)) - lines.length, lines, pal, flip);
  /* Tela fina (canvas) copiada para a grade com origem em (ox, oy) pixels finos; flip espelha em x. */
  /* (ox, oy) em pixels finos da grade; o desenho está na grade de 2 por px lógico e, em R = 4, sai ampliado pelo Scale2x. */
  const fine = (ox, oy, cv, flip) => {
    const m = R === 4 ? upscale(cv.m) : cv.m;
    Object.keys(m).forEach(y => Object.keys(m[y]).forEach(x => paint(g, flip ? ox - +x - 1 : ox + +x, oy + +y, 1, 1, m[y][x])));
  };
  const px = (x, y, w, h, c) => paint(g, Math.round(x * R), Math.round(y * R), Math.round(w * R), Math.round(h * R), c);
  return { top: top, faceY: faceY, faceX: faceX, box: box, glow: glow, fine: fine, at: at, map: (x, y, lines, pal, flip) => stamp(g, x, y, lines, pal, flip), px: px };
}
/* Os mesmos desenhos na moldura local de uma estação (o espelho 'se' troca os eixos e vira os carimbos). */
function kit(t, g) {
  const rect = (u0, v0, u1, v1) => { const a = g.w(u0, v0), b = g.w(u1, v1); return [Math.min(a[0], b[0]), Math.min(a[1], b[1]), Math.max(a[0], b[0]), Math.max(a[1], b[1])]; };
  return {
    box: (u0, v0, u1, v1, z0, z1, c) => { const r = rect(u0, v0, u1, v1); t.box(r[0], r[1], r[2], r[3], z0, z1, c); },
    top: (u0, v0, u1, v1, z, c) => { const r = rect(u0, v0, u1, v1); t.top(r[0], r[1], r[2], r[3], z, c); },
    /* Face vertical virada para +u (a tela do monitor) e face virada para +v (a frente da mesa). */
    side: (u, v0, v1, z0, z1, c) => (g.se ? t.faceY(g.fy + u, g.fx + v0, g.fx + v1, z0, z1, c) : t.faceX(g.fx + u, g.fy + v0, g.fy + v1, z0, z1, c)),
    front: (v, u0, u1, z0, z1, c) => (g.se ? t.faceX(g.fx + v, g.fy + u0, g.fy + u1, z0, z1, c) : t.faceY(g.fy + v, g.fx + u0, g.fx + u1, z0, z1, c)),
    at: (u, v, z, lines, pal) => { const p = g.w(u, v); t.at(p[0], p[1], z, lines, pal, g.se); },
    /* Mapa relativo ao quadril da pessoa (dx, dy em px), espelhado em 'se'. */
    body: (dx, dy, lines, pal) => t.map(g.seat[0] + (g.se ? -dx - lines[0].length : dx), g.seat[1] + dy, lines, pal, g.se)
  };
}
/* Grade -> bitmap. Cada sprite vira um PNG do tamanho exato dos seus pixels finos (data URL, decodificado uma vez
   pelo navegador), desenhado como <image> com image-rendering: pixelated: os mesmos pixels de antes, mas repintar
   um quadro de animação é copiar um bitmap em vez de rasterizar milhares de retângulos de <path>.
   mask: as células de px lógico ocupadas, a partir de (mx, my) com largura mw (máscara de clique da mesa). */
const RGBA = {};
const rgba = c => RGBA[c] || (RGBA[c] = rgb(c).concat(c.length > 7 ? parseInt(c.slice(7, 9), 16) : 255));
let ENC = null;
/* Densidade: pixels de tela por pixel fino (escala x devicePixelRatio / R). Inteira, o bitmap fica na grade fina e o
   navegador amplia sem suavizar. Fracionária (1,5 numa tela 1x na escala 3), ampliar faria o WebKit arredondar
   diferente a cada repintura parcial e o piso "tremeria" perto das animações; então o bitmap já sai na grade da
   tela (cada coluna de tela pega o pixel fino do seu centro) e é desenhado 1:1. K = 1 marca o caso inteiro. */
let K = 1, TAG = '2@1';
/* Ao trocar de densidade (outro monitor, zoom, escala) os bitmaps da anterior saem da memória: só uma fica guardada. */
function density(scale, dpr) {
  const r = scale * dpr >= 4 ? 4 : 2, k = (scale * dpr) / r, next = Math.abs(k - Math.round(k)) < 1e-6 ? 1 : k, tag = r + '@' + next;
  if (tag === TAG) return;
  Object.keys(MEMO).forEach(key => {
    if (!key.endsWith('@' + TAG)) return;
    if (key.startsWith('art:') && MEMO[key].href) URL.revokeObjectURL(MEMO[key].href);
    delete MEMO[key];
  });
  sceneryCache.clear();
  R = r; K = next; TAG = tag;
}
/* PNG -> blob: URL. Curta (a marcação de cada mesa não carrega KB de base64 a cada troca de estado) e a mesma
   URL para o mesmo desenho: o navegador reaproveita a imagem já decodificada quando a mesa é refeita. */
function blobUrl(canvas) {
  const b64 = canvas.toDataURL('image/png').slice(22), bin = atob(b64), bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  return URL.createObjectURL(new Blob([bytes], { type: 'image/png' }));
}
function toImage(g) {
  let bx0 = 1e9, bx1 = -1e9, by0 = 1e9, by1 = -1e9;
  const ys = Object.keys(g).map(Number);
  ys.forEach(y => { by0 = Math.min(by0, y); by1 = Math.max(by1, y + 1); Object.keys(g[y]).forEach(xs => { const x = +xs; if (x < bx0) bx0 = x; if (x + 1 > bx1) bx1 = x + 1; }); });
  /* Sem pixel nenhum (grade vazia, ou só linhas criadas por pinturas de largura 0): sprite vazio. */
  if (bx1 <= bx0) return { href: '', x: 0, y: 0, w: 0, h: 0, box: [0, 0, 0, 0], mx: 0, my: 0, mw: 0, mask: new Uint8Array(0) };
  const mx = Math.floor(bx0 / R), my = Math.floor(by0 / R), mw = Math.ceil(bx1 / R) - mx, mask = new Uint8Array(mw * (Math.ceil(by1 / R) - my));
  ys.forEach(y => { const mo = (Math.floor(y / R) - my) * mw - mx; Object.keys(g[y]).forEach(xs => { mask[mo + Math.floor(+xs / R)] = 1; }); });
  /* Grade do bitmap: a fina (K = 1) ou a da tela; [x0, x1) x [y0, y1) em pixels dela. */
  const k = K, x0 = Math.floor(bx0 * k), x1 = Math.ceil(bx1 * k), y0 = Math.floor(by0 * k), y1 = Math.ceil(by1 * k);
  const w = x1 - x0, h = y1 - y0, data = new Uint8ClampedArray(w * h * 4);
  for (let Y = y0; Y < y1; Y++) {
    const row = g[k === 1 ? Y : Math.floor((Y + 0.5) / k)]; if (!row) continue;
    for (let X = x0; X < x1; X++) {
      const cs = row[k === 1 ? X : Math.floor((X + 0.5) / k)]; if (!cs) continue;
      const c = rgba(cs), i = ((Y - y0) * w + X - x0) * 4;
      data[i] = c[0]; data[i + 1] = c[1]; data[i + 2] = c[2]; data[i + 3] = c[3];
    }
  }
  if (!ENC) ENC = document.createElement('canvas');
  ENC.width = w; ENC.height = h;
  ENC.getContext('2d').putImageData(new ImageData(data, w, h), 0, 0);
  /* De volta a px lógicos: um px lógico tem R * k pixels do bitmap. */
  const u = R * k, box = [x0 / u, y0 / u, w / u, h / u];
  return { href: blobUrl(ENC), x: box[0], y: box[1], w: box[2], h: box[3], box: box, mx: mx, my: my, mw: mw, mask: mask };
}
/* Sprite = um <g>: classes de animação, estilo inline e o bitmap (memoizado pela chave do desenho).
   SO é o mesmo sprite com contorno: móveis, pessoas e objetos soltos (telas, brilhos e fumaça ficam sem). */
function S(key, cls, style, draw, ol) { return { k: key, cls: cls || '', style: style || '', art: memo('art:' + key + '@' + TAG, () => { const g = {}; draw(iso(g)); if (ol) outline(g); return toImage(g); }) }; }
const SO = (key, cls, style, draw) => S(key, cls, style, draw, true);
/* Quadros alternados por visibilidade: n sprites, cada um visível em 1/n do ciclo. */
function frames(n, dur, make) {
  const out = [];
  for (let i = 0; i < n; i++) {
    const s = make(i);
    s.cls = 'fr fr' + n + (i === 0 ? ' fr0' : '') + (s.cls ? ' ' + s.cls : '');
    s.style = 'animation-duration:' + dur + 's;animation-delay:' + (dur * i / n).toFixed(3) + 's';
    out.push(s);
  }
  return out;
}

/* Você: retrato em pixel art desenhado à mão, um caractere por pixel fino (sem Scale2x); '.' é vazio.
   Cabeça em 3/4 olhando para a esquerda, como as outras: cabelo castanho curto com as laterais baixas (degradê),
   barba curta acobreada cobrindo bochechas, bigode e queixo, sorriso aberto com os dentes e orelhas rosadas.
   Camisa do Grêmio: xadrez de blocos grandes azul-celeste e branco, gola careca azul, escudo e três estrelas. */
const YOU = {
  pal: {
    /* cabelo: sombra, base, luz, brilho; degradê das laterais, do escuro ao claro perto da orelha */
    d: '#30231d', H: '#4a372d', h: '#68503f', y: '#8a6f5b', F: '#6a5244', f: '#94776a', g: '#bb9b88',
    /* pele: luz, base, sombra, sombra funda, rubor */
    L: '#fcdcc4', S: '#f2c6a6', s: '#dca383', z: '#c3866a', r: '#eda48e',
    /* barba: luz, base, sombra, fios sobre a pele */
    b: '#c98652', B: '#a9663b', D: '#844a2b', t: '#dba07a',
    /* sobrancelha, olho fechado, dentes, boca; q é a sombra da franja na testa */
    e: '#5e3d2c', E: '#3a2620', W: '#fbf8f1', m: '#6a2723', q: '#e9b393',
    /* olhos abertos */
    o: '#1a1b20', i: '#f4f5f7',
    /* camisa: branco e sombra, celeste e sombra, gola; escudo (anel, faixa, azul) e estrelas (bronze, prata, ouro) */
    w: '#f3f6f9', v: '#d0dae5', C: '#a9d3f2', c: '#80b0d9', K: '#2f7fc4',
    N: '#2c3a5c', X: '#15171d', x: '#3f95e0', '1': '#d08c50', '2': '#e8ebf0', '3': '#f2c44c'
  },
  /* Cabeça, linhas cy-9 (topo do cabelo) a cy+10 (pescoço, até a gola), colunas cx-9 a cx+7. A luz vem do alto à
     esquerda: franja com sombra na testa, lado do rosto e nuca mais escuros, orelha em (cx+3..cx+5, cy-1..cy+3).
     Nos olhos fica a linha do olho fechado (E): é o que aparece quando a camada dos olhos some, na piscada. */
  head: [
    '.....hyyhH.......',
    '...hhyhHhhHd.....',
    '.hhyhHHhyhHHdd...',
    '.hHHhHHhHHdHHdd..',
    'HHdHHHdHHHdHHddd.',
    'dHHdHHHdHHdFFFFFF',
    '.qqqqqqqqqFfffFFF',
    '.SLLLSSSSsgffffFF',
    'SeeeSSeeeesggrrgF',
    'SSSSSSSSSstgrzrfF',
    'SSSSSsSSSStBrzrfF',
    'SSEESsSEESBtSrsgF',
    'SrrSLzSrrtBBzsgF.',
    'ttbbBBBBttBBDDgf.',
    'bmWWWWWmBBBDDzg..',
    'bBBmmmBBBBDDzz...',
    '.bbBBBBBBDDzzs...',
    '..DBBBBDDDzzs....',
    '.......zzzzss....',
    '.......zssss.....'
  ],
  /* Camadas do rosto, com a mesma origem da cabeça: olhos abertos com brilho (linhas cy+1 e cy+2), olhos fechados
     sorrindo (^ ^ sobre a linha do olho fechado) e, na falha, dentes cerrados por cima do sorriso. */
  eyes: ['', '', '', '', '', '', '', '', '', '', '..io....io', '..oo....oo'],
  happy: ['', '', '', '', '', '', '', '', '', '', '..E....EE', '.ESE..ESSE'],
  grimace: ['', '', '', '', '', '', '', '', '', '', '', '', '', '', '.mWWWWWm', 'BmWWWWWmB'],
  /* Camisa, de top para baixo, colunas cx-9 a cx+8 (a mesa cobre a parte de baixo). Blocos: em cima branco, celeste,
     branco (com as três estrelas e o escudo) e celeste; embaixo invertido. Lado direito na sombra. */
  shirt: [
    '...wCKsssssKwwC...',
    '.wwwCCKsssKwwwCcc.',
    'wwwwCCCKKKwwwwCccc',
    'wwwwCCC1w2w3wwCccc',
    'wwwwCCCwwwwwwwCccc',
    'wwwwCCCwNNNwwwCccc',
    'wwwwCCCNxWxNwwCccc',
    'wwwwCCCNXXXNwwCccc',
    'wwwwCCCNxWxNwwCccc',
    'wwwwCCCwNNNwwwCccc',
    'CCCCwwwCCCCCCCwvvv',
    'CCCCwwwCCCCCCCwvvv',
    'CCCCwwwCCCCCCCwvvv',
    'CCCCwwwCCCCCCCwvvv',
    'CCCCwwwCCCCCCCwvvv',
    'CCCCwwwCCCCCCCwvvv',
    'CCCCwwwCCCCCCCwvvv',
    'CCCCwwwCCCCCCCwvvv',
    'CCCCwwwCCCCCCCwvvv',
    'CCCCwwwCCCCCCCwvvv',
    'CCCCwwwCCCCCCCwvvv',
    'CCCCwwwCCCCCCCwvvv',
    'CCCCwwwCCCCCCCwvvv'
  ]
};
/* Copia um mapa desenhado à mão para a tela fina a partir de (x0, y0). */
function blit(cv, x0, y0, rows, pal) {
  rows.forEach((ln, j) => { for (let i = 0; i < ln.length; i++) { const c = pal[ln[i]]; if (c) cv.set(x0 + i, y0 + j, c); } });
}

/* Pessoas: uma por papel, com a roupa na cor do papel, desenhadas direto na grade fina (2x2 por px lógico) a partir
   de formas: cabeça em 3/4 virada para a esquerda (olha para sudoeste; o espelho dá sudeste), tronco com gola e um
   detalhe do papel, e braços como membros de verdade (ombro -> cotovelo -> mão), sempre presos ao ombro.
   pal: S/s pele e sombra, H/h cabelo e brilho, C/c roupa e sombra, P calça. hair: estilo do cabelo; extra: acessório. */
const PEOPLE = {
  triage: { pal: { S: '#8d5a3b', s: '#6f4329', H: '#1c1a1e', h: '#3a3540', C: '#49b5b0', c: '#2c7f7b', P: '#3d4560' }, hair: 'curly', shirt: 'lanyard' },
  planner: { pal: { S: '#f1c9a5', s: '#d9a982', H: '#b5532f', h: '#d9784a', C: '#6f7de0', c: '#4450a8', P: '#6b5a4a' }, hair: 'long', shirt: 'cardigan' },
  implementer: { pal: { S: '#c68c5f', s: '#a8714a', H: '#2b2320', h: '#4a3a33', C: '#e0894e', c: '#a85f30', P: '#55607d' }, hair: 'short', extra: 'headset', shirt: 'hoodie' },
  reviewer: { pal: { S: '#f7dcc4', s: '#e0bea2', H: '#c9ccd4', h: '#eef0f3', C: '#dd6a83', c: '#9c4257', P: '#5d6270' }, hair: 'bun', extra: 'glasses', shirt: 'pearls' },
  tester: { pal: { S: '#e0ac7e', s: '#c48f62', H: '#15161a', h: '#34363e', C: '#8fc45a', c: '#5c8a35', P: '#4a5578' }, hair: 'short', extra: 'goggles', shirt: 'coat' },
  reporter: { pal: { S: '#a9714b', s: '#8a5a3a', H: '#3a2a20', h: '#5a4232', C: '#b377d9', c: '#7d4a9e', P: '#6b5a4a' }, hair: 'beanie', shirt: 'scarf' },
  /* Você, na mesa de aprovação: retrato desenhado à mão (YOU), no lugar das formas. hair fica para a vista de costas. */
  approval: { pal: { S: '#f2c6a6', s: '#dca383', H: '#5e4030', h: '#82604a', C: '#a9d3f2', c: '#7fb0d8', P: '#2f3646' }, hair: 'fade', shirt: 'gremio', art: YOU },
  /* Code review: coque no alto, óculos de leitura e camisa âmbar com gravata. */
  code_review: { pal: { S: '#d29a6e', s: '#b27d53', H: '#4a2c22', h: '#7a4c3a', C: '#d9a441', c: '#9c7226', P: '#3f4a63' }, hair: 'topknot', extra: 'readers', shirt: 'tie' }
};
const EYE = '#1a1b20', WHITE = '#f4f5f7';

/* Tela fina ampliada 2x pelo Scale2x (cores como símbolos): as curvas e diagonais ganham degraus finos. */
function upscale(m) {
  const ys = Object.keys(m).map(Number);
  if (!ys.length) return {};
  let x0 = 1e9, x1 = -1e9;
  ys.forEach(y => Object.keys(m[y]).forEach(xs => { x0 = Math.min(x0, +xs); x1 = Math.max(x1, +xs); }));
  const y0 = Math.min.apply(null, ys), y1 = Math.max.apply(null, ys), src = [];
  for (let y = y0; y <= y1; y++) { const r = []; for (let x = x0; x <= x1; x++) r.push((m[y] && m[y][x]) || '.'); src.push(r); }
  const up = scale2x(src), out = {};
  up.forEach((r, j) => r.forEach((c, i) => { if (c !== '.') (out[y0 * 2 + j] || (out[y0 * 2 + j] = {}))[x0 * 2 + i] = c; }));
  return out;
}
/* Volume por parte do corpo: a luz vem do alto à esquerda da tela. Cada pixel mede quanto do desenho da própria
   parte há até a borda virada para a luz e até a oposta, na horizontal e na vertical; isso dá a posição dele na forma
   (0 na borda iluminada, 1 na da sombra), como numa forma arredondada. Daí saem quatro faixas de tom (brilho, base,
   sombra e sombra funda), com pontilhado só na passagem de uma para a outra, como na pixel art sombreada à mão.
   wx, wy: quanto a luz vem de lado e de cima nessa parte. No desenho espelhado a luz troca de lado no espaço local. */
function shapeLight(cv, wx, wy, bias, noise) {
  const m = cv.m, L = cv.flip ? 1 : -1, has = (x, y) => !!(m[y] && m[y][x] && m[y][x].length <= 7), out = [];
  const run = (x, y, dx, dy) => { let k = 1; while (k < 24 && has(x + dx * k, y + dy * k)) k++; return k; };
  Object.keys(m).forEach(ys => Object.keys(m[ys]).forEach(xs => {
    const x = +xs, y = +ys, c = m[y][x]; if (c.length > 7) return;
    const a = run(x, y, L, 0), b = run(x, y, -L, 0), up = run(x, y, 0, -1), dn = run(x, y, 0, 1);
    const u = (a - 0.5) / Math.max(1, a + b - 1), v = (up - 0.5) / Math.max(1, up + dn - 1);
    const n = (0.5 - u) * wx + (0.5 - v) * wy + bias + (bayer(x, y) - 0.5) * noise;
    const t = n > 0.3 ? 0.13 : n > -0.08 ? 0 : n > -0.38 ? -0.13 : -0.24;
    if (t) out.push([x, y, tone(c, t)]);
  }));
  out.forEach(o => { m[o[1]][o[0]] = o[2]; });
}
/* Desenha uma parte do corpo numa tela própria (que enxerga a de baixo, para o pescoço não cobrir a gola), dá volume
   a ela e a assenta na tela de baixo, projetando sombra de `cast` px para baixo e para o lado oposto à luz sobre o
   que já estava lá (o braço no tronco, a cabeça no pescoço), mais forte junto da borda. Onde a parte encosta em
   outra, a borda de cima ganha um fio de luz e a de baixo um de sombra: as duas não se fundem mesmo com a mesma cor.
   flat: desenho à mão, já sombreado. skin: cores de pele da parte; a sombra que cai depois sobre esses pixels (a mão
   de baixo nos braços cruzados) fica bem mais leve, e a própria parte só escurece a pele um degrau leve: a mão não
   vira uma mancha escura. */
function part(cv, draw, wx, wy, cast, flat, bias, noise, skin) {
  const sub = canvas(cv.flip, cv);
  draw(sub);
  const bare = {};
  if (skin) Object.keys(sub.m).forEach(ys => Object.keys(sub.m[ys]).forEach(xs => { const c = sub.m[ys][xs]; if (skin.has(c)) { cv.soft[ys + ',' + xs] = 1; bare[ys + ',' + xs] = c; } }));
  const L = cv.flip ? 1 : -1, own = (x, y) => !!(sub.m[y] && sub.m[y][x]), dark = {};
  const under = (x, y) => { const c = cv.m[y] && cv.m[y][x]; return !!c && c.length <= 7 && !own(x, y); };
  if (!flat) {
    shapeLight(sub, wx, wy, bias || 0, noise == null ? 0.12 : noise);
    const edge = [];
    Object.keys(sub.m).forEach(ys => Object.keys(sub.m[ys]).forEach(xs => {
      const x = +xs, y = +ys, c = sub.m[y][x]; if (c.length > 7) return;
      let d = 0;
      if (under(x, y + 1) || under(x - L, y)) d -= 0.12;
      else if (under(x, y - 1) || under(x + L, y)) d += 0.1;
      if (d) edge.push([x, y, tone(c, d)]);
    }));
    edge.forEach(e => { sub.m[e[1]][e[0]] = e[2]; });
    const sum = c => rgb(c).reduce((a, b) => a + b, 0);
    Object.keys(bare).forEach(key => {
      const xy = key.split(','), y = +xy[0], x = +xy[1], c = sub.m[y][x], o = bare[key];
      if (sum(c) < sum(o)) sub.m[y][x] = tone(o, -0.07);
    });
  }
  Object.keys(sub.m).forEach(ys => Object.keys(sub.m[ys]).forEach(xs => {
    for (let k = 1; k <= cast; k++) {
      const x = +xs - L * Math.ceil(k / 2), y = +ys + k, key = y + ',' + x;
      if (under(x, y) && !(key in dark && dark[key][3] <= k)) dark[key] = [x, y, cv.m[y][x], k];
    }
  }));
  Object.keys(dark).forEach(key => {
    const d = dark[key], soft = cv.soft[key];
    if (!soft || d[3] === 1) cv.m[d[1]][d[0]] = tone(d[2], soft ? -0.07 : d[3] === 1 ? -0.24 : -0.15);
  });
  Object.keys(sub.m).forEach(ys => Object.keys(sub.m[ys]).forEach(xs => cv.set(+xs, +ys, sub.m[ys][xs])));
}
/* Tela fina: pixels soltos em coordenadas locais (origem no quadril), copiados depois para a grade com espelho. */
function canvas(flip, under) {
  const m = {};
  const set = (x, y, c) => { if (c === null) { if (m[y]) delete m[y][x]; } else if (c) (m[y] || (m[y] = {}))[x] = c; };
  const get = (x, y) => (m[y] && m[y][x]) || (under && under.get(x, y));
  /* Elipse amostrada no centro do pixel; keep(x, y) filtra. */
  const ell = (cx, cy, rx, ry, c, keep) => {
    for (let y = Math.floor(cy - ry); y <= Math.ceil(cy + ry); y++) for (let x = Math.floor(cx - rx); x <= Math.ceil(cx + rx); x++) {
      const dx = (x + 0.5 - cx) / rx, dy = (y + 0.5 - cy) / ry;
      if (dx * dx + dy * dy <= 1 && (!keep || keep(x, y))) set(x, y, typeof c === 'function' ? c(x, y) : c);
    }
  };
  const rect = (x0, y0, x1, y1, c) => { for (let y = y0; y <= y1; y++) for (let x = x0; x <= x1; x++) set(x, y, c); };
  /* Traço grosso por uma linha de pontos (membros): discos de raio r ao longo de cada trecho. */
  const limb = (pts, r, c) => {
    for (let i = 0; i + 1 < pts.length; i++) {
      const a = pts[i], b = pts[i + 1], n = Math.max(1, Math.ceil(Math.hypot(b[0] - a[0], b[1] - a[1]) * 3));
      for (let k = 0; k <= n; k++) ell(a[0] + (b[0] - a[0]) * k / n, a[1] + (b[1] - a[1]) * k / n, r, r, c);
    }
  };
  return { m: m, soft: {}, flip: !!flip, set: set, get: get, ell: ell, rect: rect, limb: limb };
}

/* Cabeça em 3/4 (olhando para a esquerda) com centro do crânio em (cx, cy). back: vista de costas (só cabelo,
   orelha e nuca). Os olhos ficam fora: são uma camada própria, para piscar. */
/* Cabeça: forma redonda (luz de lado e de cima), com sombra no pescoço e na gola. */
function drawHead(cv, p, cx, cy, back) { part(cv, s => headShape(s, p, cx, cy, back), 0.75, 0.55, 2, !!p.art && !back, 0.22, 0.04); }
function headShape(cv, p, cx, cy, back) {
  if (p.art && !back) { blit(cv, cx - 9, cy - 9, p.art.head, p.art.pal); return; }
  const S = p.pal.S, s = p.pal.s, H = p.pal.H, h = p.pal.h, Hd = shade(H, 0.35);
  /* Pescoço e crânio: calota larga em cima e queixo mais estreito, puxado para a frente. */
  /* O pescoço só preenche o vão até o tronco (não cobre a gola). */
  for (let y = cy + 5; y <= cy + 11; y++) for (let x = cx - 3; x <= cx + 1; x++) if (!cv.get(x, y)) cv.set(x, y, s);
  const skull = (x, y) => {
    const a = (x + 0.5 - cx) / 8, b = (y + 0.5 - cy + 0.5) / 7.5, j = (x + 0.5 - cx + 0.5) / 6.5, k = (y + 0.5 - cy - 2.5) / 5;
    return a * a + b * b <= 1 || j * j + k * k <= 1;
  };
  for (let y = cy - 9; y <= cy + 8; y++) for (let x = cx - 9; x <= cx + 9; x++) if (skull(x, y)) cv.set(x, y, x >= cx + 4 || y >= cy + 7 ? s : S);
  /* Cabelo: tudo da calota acima da linha do cabelo; atrás da orelha desce até a nuca. hl(x) = linha do cabelo. */
  const st = p.hair;
  const cap = (hl, grow) => {
    for (let y = cy - 10; y <= cy + 14; y++) for (let x = cx - 10; x <= cx + 10; x++) {
      const a = (x + 0.5 - cx) / (8 + grow), b = (y + 0.5 - cy + 0.8) / (7.6 + grow);
      if (a * a + b * b <= 1 && (back || y < hl(x))) cv.set(x, y, H);
    }
  };
  const fringe = x => (x >= cx + 2 ? cy + 4 : x <= cx - 7 ? cy + 1 : cy - 2 - ((x + 40) % 3 === 0 ? 1 : 0));
  if (st === 'curly') {
    cap(fringe, 1);
    for (let i = 0; i < 9; i++) { const th = Math.PI * (0.95 + i * 0.13); cv.ell(cx + Math.cos(th) * 8.6, cy - 1 + Math.sin(th) * 8, 1.7, 1.7, H); }
  } else if (st === 'long') {
    cap(x => (x >= cx + 1 ? cy + 13 : x <= cx - 7 ? cy + 6 : cy - 2 + (x > cx - 3 ? 0 : 1)), 1.2);
    cv.rect(cx + 2, cy + 4, cx + 9, cy + 13, H);
    for (let y = cy + 9; y <= cy + 13; y++) cv.set(cx + 9 - ((y - cy) % 2), y, null);
  } else if (st === 'bun' || st === 'topknot') {
    cap(fringe, 0.6);
    if (st === 'bun') cv.ell(cx + 5, cy - 6, 3.4, 3.2, H); else cv.ell(cx, cy - 10, 3.2, 2.8, H);
  } else if (st === 'beanie') {
    cap(fringe, 0.4);
    const C = p.pal.C, c = p.pal.c;
    cv.ell(cx, cy - 3, 9, 7, C, (x, y) => y <= cy - 2);
    cv.rect(cx - 9, cy - 3, cx + 9, cy - 2, c);
    for (let x = cx - 8; x <= cx + 8; x += 2) cv.set(x, cy - 3, shade(c, 0.2));
    cv.ell(cx + 1, cy - 10, 2, 1.8, WHITE);
  } else if (st === 'fade') {
    /* Topete curto e laterais raspadas, mais claras perto da orelha. */
    cap(x => (x >= cx + 2 ? cy + 2 : x <= cx - 7 ? cy - 1 : cy - 3), 0.4);
    const fade = mix(H, S, 0.45);
    for (let y = cy - 2; y <= cy + 3; y++) for (let x = cx - 10; x <= cx + 10; x++) if (cv.get(x, y) === H) cv.set(x, y, fade);
  } else cap(fringe, 0.5);
  /* Brilho do cabelo: um arco claro no alto da cabeça. */
  if (st !== 'beanie') for (let x = cx - 4; x <= cx + 3; x++) { const y = Math.round(cy - 6.6 + Math.pow((x - cx + 0.5) / 4.5, 2) * 2); if (cv.get(x, y) === H) cv.set(x, y, h); }
  /* Orelha atrás da bochecha (de costas, dos dois lados). */
  const ear = ex => { cv.ell(ex, cy + 1.5, 1.7, 2.3, S); cv.set(Math.floor(ex), cy + 1, s); cv.set(Math.floor(ex), cy + 2, s); };
  if (st !== 'long') ear(cx + 4.5); if (back && st !== 'long') ear(cx - 6.5);
  if (!back) {
    /* Sobrancelhas, nariz, boca e bochechas. Os olhos ficam em (cx-6, cy+1) e (cx-1, cy+1). */
    cv.rect(cx - 7, cy - 1, cx - 5, cy - 1, Hd); cv.rect(cx - 2, cy - 1, cx, cy - 1, Hd);
    cv.set(cx - 4, cy + 3, s); cv.set(cx - 5, cy + 4, s);
    cv.rect(cx - 6, cy + 6, cx - 4, cy + 6, mix(S, '#6a2420', 0.55));
    cv.set(cx - 8, cy + 4, mix(S, '#ff7a7a', 0.3)); cv.set(cx + 1, cy + 4, mix(S, '#ff7a7a', 0.3)); cv.set(cx + 2, cy + 4, mix(S, '#ff7a7a', 0.3));
  }
  /* Acessórios. */
  const ex = p.extra;
  if (ex === 'headset') {
    for (let i = 0; i <= 12; i++) { const th = Math.PI * (1.05 + i * 0.075); cv.set(Math.round(cx + 0.5 + Math.cos(th) * 9), Math.round(cy + Math.sin(th) * 8.6), '#5a5f69'); }
    cv.ell(cx + 4.5, cy + 1.5, 2.6, 3.2, '#3d424d'); cv.ell(cx + 4.2, cy + 1.2, 1.2, 1.6, '#e0894e');
    if (!back) { cv.rect(cx - 2, cy + 6, cx + 3, cy + 6, '#3d424d'); cv.set(cx - 3, cy + 6, '#e0894e'); }
  } else if (ex === 'goggles') {
    cv.rect(cx - 8, cy - 4, cx + 8, cy - 3, '#5b6ad9');
    if (!back) { cv.rect(cx - 8, cy - 5, cx - 5, cy - 2, '#a6b0f7'); cv.rect(cx - 3, cy - 5, cx, cy - 2, '#a6b0f7'); cv.set(cx - 7, cy - 4, WHITE); cv.set(cx - 2, cy - 4, WHITE); }
  } else if ((ex === 'glasses' || ex === 'readers') && !back) {
    const fr = ex === 'glasses' ? '#3a3e48' : '#7a3b2a';
    [cx - 6, cx - 1].forEach(x => { cv.rect(x - 2, cy - 1, x + 2, cy - 1, fr); cv.rect(x - 2, cy + 3, x + 2, cy + 3, fr); cv.rect(x - 2, cy, x - 2, cy + 2, fr); cv.rect(x + 2, cy, x + 2, cy + 2, fr); });
    cv.set(cx - 3, cy, fr); cv.rect(cx + 2, cy, cx + 4, cy, fr);
  }
}
/* Olhos (camada própria): abertos com brilho, ou fechados e sorrindo. */
function drawEyes(cv, p, cx, cy, happy) {
  if (p.art) { blit(cv, cx - 9, cy - 9, happy ? p.art.happy : p.art.eyes, p.art.pal); return; }
  [cx - 6, cx - 1].forEach(x => {
    if (happy) { cv.set(x - 1, cy + 2, EYE); cv.set(x, cy + 1, EYE); cv.set(x + 1, cy + 2, EYE); return; }
    cv.rect(x - 1, cy + 1, x, cy + 2, EYE); cv.set(x - 1, cy + 1, WHITE);
  });
}
/* Tronco: ombros arredondados, sombra do lado de trás, gola em V e o detalhe da roupa do papel. */
/* Tronco: cilindro de pé (luz mais de lado), escurecendo para baixo, onde a mesa cobre. */
function drawTorso(cv, p, cx, top, bottom) { part(cv, s => torsoShape(s, p, cx, top, bottom), 0.95, 0.35, 0, !!p.art); }
function torsoShape(cv, p, cx, top, bottom) {
  if (p.art && p.shirt) { blit(cv, cx - 9, top, p.art.shirt.slice(0, bottom - top + 1), p.art.pal); return; }
  const C = p.pal.C, c = p.pal.c, S = p.pal.S;
  for (let y = top; y <= bottom; y++) {
    const r = y === top ? 5 : y === top + 1 ? 7 : 8;
    for (let x = cx - r - 1; x <= cx + r; x++) cv.set(x, y, x >= cx + r - 2 ? c : x === cx - r - 1 ? lit(C, 0.18) : C);
  }
  cv.rect(cx - 3, top, cx + 1, top, S); cv.rect(cx - 2, top + 1, cx, top + 1, S); cv.set(cx - 1, top + 2, S);
  cv.set(cx - 4, top, lit(C, 0.3)); cv.set(cx + 2, top, lit(C, 0.3));
  const sh = p.shirt;
  if (sh === 'lanyard') { for (let y = top; y <= top + 6; y++) cv.set(cx - 4 + Math.floor((y - top) / 3), y, '#e3ba6a'); cv.rect(cx - 4, top + 7, cx - 1, top + 10, WHITE); cv.rect(cx - 3, top + 8, cx - 2, top + 8, '#5b6ad9'); }
  else if (sh === 'cardigan') { for (let y = top + 3; y <= bottom; y++) cv.set(cx - 1, y, c); for (let y = top + 4; y <= bottom; y += 3) cv.set(cx - 2, y, WHITE); }
  else if (sh === 'hoodie') { cv.rect(cx + 1, top - 1, cx + 6, top, c); cv.rect(cx - 3, top + 2, cx - 3, top + 6, WHITE); cv.rect(cx, top + 2, cx, top + 5, WHITE); cv.rect(cx - 5, top + 9, cx + 3, top + 9, c); }
  else if (sh === 'pearls') { [[-4, 1], [-3, 2], [-1, 3], [1, 2], [2, 1]].forEach(d => cv.set(cx + d[0], top + d[1], WHITE)); }
  else if (sh === 'coat') { for (let y = top; y <= bottom; y++) { cv.set(cx - 5, y, WHITE); cv.set(cx - 4, y, '#d8dbe2'); cv.set(cx + 3, y, '#d8dbe2'); cv.set(cx + 4, y, WHITE); } cv.rect(cx + 5, top + 5, cx + 6, top + 6, '#5b6ad9'); }
  else if (sh === 'scarf') { cv.rect(cx - 5, top, cx + 4, top + 2, '#e3ba6a'); cv.rect(cx - 4, top + 3, cx - 2, top + 8, '#e3ba6a'); for (let y = top; y <= top + 8; y += 2) cv.set(cx - 3, y, '#d9645a'); }
  else if (sh === 'tie') { cv.rect(cx - 2, top + 2, cx - 1, top + 9, '#3f4a63'); cv.set(cx - 2, top + 10, '#3f4a63'); cv.rect(cx - 4, top, cx - 3, top + 1, WHITE); cv.rect(cx, top, cx + 1, top + 1, WHITE); }
}
/* Braço: manga do ombro ao cotovelo e até o pulso, mão na ponta; opcionalmente algo na mão (caneca). */
/* Braço: membro fino, iluminado por cima; projeta sombra no tronco. */
function drawArm(cv, p, pts) { part(cv, s => armShape(s, p, pts), 0.6, 0.8, 2, !!p.art, 0.05, 0.06, new Set([p.pal.S, p.pal.s])); }
function armShape(cv, p, pts) {
  if (p.art) { armYou(cv, p.art.pal, pts); return; }
  const C = p.pal.C, c = p.pal.c, S = p.pal.S, s = p.pal.s, end = pts[pts.length - 1];
  cv.limb(pts, 2.1, C);
  /* Sombra da manga: o lado de baixo de cada trecho. */
  cv.limb(pts.map(q => [q[0] + 0.6, q[1] + 0.9]), 1.1, (x, y) => (cv.get(x, y) === C && !(cv.get(x, y + 2) === C) ? c : cv.get(x, y)));
  cv.ell(end[0], end[1], 1.9, 1.8, S); cv.set(Math.round(end[0]), Math.round(end[1]) + 1, s);
}

/* Braço do usuário, pixel a pixel: antebraço de pele (cápsula por trecho) e manga curta do ombro até perto do
   cotovelo, em três faixas como na camisa (celeste, branco, barra celeste). A luz vem do alto à esquerda: o lado
   oposto de cada trecho fica na sombra, em qualquer direção. Braço levantado termina em mão aberta. */
function armYou(cv, A, pts) {
  const sh = pts[0], el = pts[1], end = pts[pts.length - 1], LX = -0.6, LY = -0.8;
  /* Ponto mais próximo de (px, py) no trecho a-b: distância, deslocamento (ox, oy) e t ao longo do trecho. */
  const near = (a, b, px, py) => {
    const dx = b[0] - a[0], dy = b[1] - a[1], l2 = dx * dx + dy * dy || 1;
    const t = Math.max(0, Math.min(1, ((px - a[0]) * dx + (py - a[1]) * dy) / l2)), ox = px - a[0] - dx * t, oy = py - a[1] - dy * t;
    return { d: Math.hypot(ox, oy), lit: ox * LX + oy * LY, t: t };
  };
  const xs = pts.map(q => q[0]), ys = pts.map(q => q[1]);
  const x0 = Math.floor(Math.min.apply(null, xs)) - 3, x1 = Math.ceil(Math.max.apply(null, xs)) + 3;
  const y0 = Math.floor(Math.min.apply(null, ys)) - 3, y1 = Math.ceil(Math.max.apply(null, ys)) + 3;
  const L1 = Math.hypot(el[0] - sh[0], el[1] - sh[1]), cuff = 0.72;
  for (let y = y0; y <= y1; y++) for (let x = x0; x <= x1; x++) {
    const px = x + 0.5, py = y + 0.5;
    let best = null;
    for (let i = 0; i + 1 < pts.length; i++) { const q = near(pts[i], pts[i + 1], px, py); if (!best || q.d < best.d) best = q; }
    /* Manga: só no trecho ombro-cotovelo, até cuff do comprimento (corte reto, perpendicular ao braço). */
    const u = near(sh, el, px, py), along = u.t * L1;
    if (u.d <= 2.4 && along <= L1 * cuff) {
      const dark = u.lit < -0.9, band = along < L1 * cuff * 0.45 || along > L1 * cuff - 1.3;
      cv.set(x, y, band ? (dark ? A.c : A.C) : dark ? A.v : A.w);
    } else if (best.d <= 1.6) cv.set(x, y, best.lit < -0.45 ? A.s : A.S);
  }
  /* Antebraço apontando para cima (aceno, alongar, mão no rosto): mão aberta, com o polegar para o lado do corpo. */
  const fx = end[0] - el[0], fy = end[1] - el[1];
  if (fy < -0.6 * Math.hypot(fx, fy)) { blit(cv, Math.round(end[0]) - 2, Math.round(end[1]) - 3, sh[0] > 0 ? HAND_OPEN : HAND_OPEN.map(r => r.split('').reverse().join('')), A); return; }
  cv.ell(end[0], end[1], 1.9, 1.8, A.S); cv.set(Math.round(end[0]), Math.round(end[1]) + 1, A.s);
}
/* Mão aberta de frente (polegar à esquerda), ancorada na palma: coluna 2, linha 3. */
const HAND_OPEN = ['..S.S', '.SSSS', '.SSSS', 'SSSSs', '.SSss'];

/* Poses dos braços (pontos finos relativos ao quadril, olhando para sudoeste). Ombros em L e R. */
const SH_L = [-8, -11], SH_R = [6, -11];
const POSE = {
  restL: [SH_L, [-11, -4], [-14, 1]],
  restR: [SH_R, [8, -3], [-3, 3]],
  crossL: [SH_L, [-10, -4], [4, -6]],
  crossR: [SH_R, [8, -3], [-6, -7]],
  sipR: [SH_R, [9, -4], [-5, -17]],
  palmL: [SH_L, [-12, -5], [-8, -22]],
  stretch: [[[SH_L, [-12, -21], [-8, -35]], [SH_R, [10, -21], [6, -35]]], [[SH_L, [-13, -23], [-10, -38]], [SH_R, [11, -23], [8, -38]]]],
  wave: [[SH_R, [12, -18], [11, -30]], [SH_R, [13, -17], [15, -28]]]
};
const HEAD_C = [-1, -25];

/* Caminhada (caixa 12x20 px lógicos, pés na última linha): quadris e ciclo de pernas nos mapas; o resto na grade fina. */
const HIPS = ['..PPPPPP..', '..PPPPPP..', '..PP..PP..'];
const STEP = ['..PP..PP..', '..PP..PP..', '..PP..PP..', '..WW..WW..'];
const LEG_FR = [STEP, ['..PP..PP..', '..PP..PP..', '..PP..WW..', '..WW......'], STEP, ['..PP..PP..', '..PP..PP..', '..WW..PP..', '......WW..']];

function walkerSprites(role, back, carry, frame) {
  const p = PEOPLE[role], still = frame != null, out = [], id = role + (back ? '-b' : '-f');
  const legs = i => SO('wk-legs-' + role + i, '', '', t => {
    /* Sombra no chão, embaixo dos pés (translúcida: o contorno não a pega). */
    const sh = canvas(); sh.ell(12, 39.6, 7, 1.7, '#05060a66'); sh.ell(12, 39.6, 4.5, 1.1, '#05060a80'); t.fine(0, 0, sh, false);
    t.map(1, 13, HIPS, p.pal); t.map(1, 16, LEG_FR[i], p.pal);
  });
  if (still) out.push(legs(frame)); else frames(4, 0.44, legs).forEach(s => out.push(s));
  const fold = carry && !back;
  out.push(SO('wk-up-' + id + (fold ? '-c' : ''), still ? '' : 'a-walkbob', '', t => {
    const cv = canvas();
    drawTorso(cv, back ? Object.assign({}, p, { shirt: '' }) : p, 12, 16, 27);
    drawHead(cv, p, 12, 8, back);
    if (fold) {
      drawArm(cv, p, [[4, 18], [3, 23], [6, 23]]); drawArm(cv, p, [[19, 18], [20, 23], [17, 23]]);
      cv.rect(5, 20, 18, 25, '#d9a441'); cv.rect(5, 19, 9, 19, '#e3ba6a'); cv.rect(7, 21, 16, 22, WHITE); cv.rect(5, 25, 18, 25, '#9c7226');
      cv.ell(6, 23, 1.8, 1.7, p.pal.S); cv.ell(17, 23, 1.8, 1.7, p.pal.S);
    } else { drawArm(cv, p, [[4, 18], [3, 23], [3, 27]]); drawArm(cv, p, [[19, 18], [20, 23], [20, 27]]); }
    if (!back) drawEyes(cv, p, 12, 8, false);
    t.fine(0, 0, cv, false);
  }));
  return out;
}

/* Pessoa sentada atrás da mesa, de frente para a câmera. body vai antes da mesa (a mesa esconde o colo);
   hands vai depois (braços que descansam no tampo). cls é a animação da camada do corpo. */
function seatedParts(role, v, i) {
  const p = PEOPLE[role], g = geo(role), id = '-' + role;
  const put = draw => t => { const cv = canvas(g.se); draw(cv); t.fine(g.seat[0] * R, g.seat[1] * R, cv, g.se); };
  const arm = pts => put(cv => drawArm(cv, p, pts));
  const body = [SO('torso' + id, '', '', put(cv => { drawTorso(cv, p, 0, -14, 8); drawHead(cv, p, HEAD_C[0], HEAD_C[1], false); }))];
  const hands = [];
  const eyes = (cls, happy) => body.push(S('eyes' + (happy ? '-happy' : '') + id, cls, '', put(cv => drawEyes(cv, p, HEAD_C[0], HEAD_C[1], happy))));
  /* Na mesa de aprovação não há caneca (o canto é do vaso). */
  const desk = role !== 'approval', sip = desk && v === 'waiting' && i % 2 === 0, d = sip ? 'animation-delay:-' + ((i * 1.9) % 7).toFixed(1) + 's' : '';
  if (desk) hands.push(mug(role, sip ? 'a-sipA' : '', d));
  let cls = 'a-bob';
  if (v === 'working') {
    cls = ''; eyes('a-wink');
    hands.push(SO('typeL' + id, 'a-tapL', '', arm(POSE.restL)));
    hands.push(SO('typeR' + id, 'a-tapR', '', arm(POSE.restR)));
  } else if (v === 'waiting') {
    eyes('a-wink');
    if (sip) {
      hands.push(SO('handL' + id, '', '', arm(POSE.restL)));
      hands.push(SO('handR' + id, 'a-sipA', d, arm(POSE.restR)));
      body.push(SO('sip' + id, 'a-sipB', d, put(cv => {
        drawArm(cv, p, POSE.sipR);
        cv.rect(-10, -21, -6, -16, WHITE); cv.rect(-10, -16, -6, -16, '#b4b8c0'); cv.rect(-5, -20, -5, -18, WHITE);
        cv.ell(-5, -17, 1.9, 1.8, p.pal.S);
      })));
    } else body.push(SO('crossed' + id, '', '', put(cv => { drawArm(cv, p, POSE.crossR); drawArm(cv, p, POSE.crossL); })));
  } else if (v === 'you') {
    eyes('a-wink');
    frames(2, 0.5, n => SO('wave' + n + id, '', '', arm(POSE.wave[n]))).forEach(s => body.push(s));
    hands.push(SO('handL' + id, '', '', arm(POSE.restL)));
  } else if (v === 'done') {
    cls = ''; eyes('', true);
    frames(2, 1.8, n => SO('stretch' + n + id, '', '', put(cv => POSE.stretch[n].forEach(a => drawArm(cv, p, a))))).forEach(s => body.push(s));
  } else if (v === 'failed') {
    cls = 'a-nod'; eyes('');
    if (p.art) body.push(S('grimace' + id, '', '', put(cv => blit(cv, HEAD_C[0] - 9, HEAD_C[1] - 9, p.art.grimace, p.art.pal))));
    body.push(SO('facepalm' + id, '', '', arm(POSE.palmL)));
    hands.push(SO('handR' + id, '', '', arm(POSE.restR)));
  } else {
    eyes('a-wink');
    hands.push(SO('handL' + id, '', '', arm(POSE.restL)));
    hands.push(SO('handR' + id, '', '', arm(POSE.restR)));
  }
  return { cls: cls, body: body, hands: hands };
}

/* Selos de estado sobre a cabeça e acessórios de mesa. */
const BADGE = {
  you: ['.AAAAAAA.', 'AAAAKAAAA', 'AAAAKAAAA', 'AAAAKAAAA', 'AAAAKAAAA', 'AAAAAAAAA', 'AAAAKAAAA', '.AAAAAAA.', '....A....'],
  done: ['.NNNNNNN.', 'NNNNNNNNN', 'NNNNNNKNN', 'NNNNNKNNN', 'NKNNKNNNN', 'NNKKNNNNN', 'NNNNNNNNN', '.NNNNNNN.'],
  returned: ['.rrrrrrr.', 'rrrrrrrrr', 'rrKrrrrrr', 'rKKKKKrrr', 'rrKrrrKrr', 'rrrrrrKrr', 'rrrKKKrrr', '.rrrrrrr.'],
  failed: ['.RRRRRRR.', 'RRRRRRRRR', 'RRWRRRWRR', 'RRRWRWRRR', 'RRRRWRRRR', 'RRRWRWRRR', 'RRWRRRWRR', '.RRRRRRR.']
};
const LUPA = ['.GGG.', 'G.b.G', 'G.b.G', '.GGG.', '...g.', '....g'];
const CLIP = ['.ggg.', 'qqqqq', 'qPPPq', 'qbbPq', 'qPPPq', 'qbPPq', 'qqqqq'];
const TUBES = ['n.W.n', 'n.W.n', 'N.n.N', 'N.N.N', 'ggggg', 'g...g'];
const MEGA = ['.RR....', 'WRRRR..', 'WRRRRRg', 'WRRRR..', '.RR....'];
const CODE = [[0, 5, 'b'], [2, 6, 'n'], [2, 4, 'w'], [4, 3, 'a'], [2, 5, 'b'], [0, 3, 'r'], [0, 6, 'n'], [2, 4, 'w']];
const PLANE = ['.......WW...', '.....WWWw...', '...WWWWw....', '.WWWWWw.....', '...WWww.....', '.....w......'];

/* Móveis da estação, na moldura local. */
function drawChair(k, you) {
  const c = you ? ['#7b88e6', '#5b6ad9', '#4450a8'] : ['#4a505c', '#3a3f4a', '#2c3038'];
  k.box(0.93, 0.47, 1.07, 0.61, 0, 5, STEEL);
  k.box(0.7, 0.26, 1.3, 0.84, 5, 7, c);
  k.box(0.72, 0.1, 1.28, 0.24, 7, you ? 21 : 18, c);
}
function drawDesk(k) {
  k.box(0.1, 1.1, 1.9, 1.84, 0, 8, WOOD_D);
  k.front(1.84, 1.24, 1.8, 1.5, 7, '#4f3c2c');
  k.front(1.84, 1.46, 1.58, 4, 5, '#b4b8c0');
  k.box(0, 1.02, 2, 1.94, 8, 10, WOOD);
}
function drawMonitor(k) {
  k.box(0.18, 1.36, 0.54, 1.64, 10, 11, STEEL);
  k.box(0.3, 1.46, 0.38, 1.54, 11, 13, STEEL);
  k.box(0.24, 1.08, 0.4, 1.92, 13, 23, CASE);
}
/* Tela: a apagada ganha um reflexo diagonal em faixa (o vidro), em pixels finos. */
const drawScreen = (k, c) => k.side(0.4, 1.14, 1.86, 14, 22, c === SCREEN || c === '#0b0c0f' ? (a, z, X, Y) => { const d = (X + Y * 2) % 30; return d < 2 || d === 4 ? tone(c, .07) : c; } : c);
function drawCode(k, n, term) {
  drawScreen(k, SCREEN);
  for (let j = 0; j < 3; j++) {
    const ln = CODE[(n + j) % CODE.length], v1 = 1.8 - ln[0] / 12;
    k.side(0.4, v1 - ln[1] / 12, v1, 19 - j * 2, 20 - j * 2, term ? PAL.N : PAL[ln[2]]);
  }
}
/* Caneca no canto da mesa: some quando a pessoa a leva à boca (a-sipA). */
const mug = (role, cls, style) => SO('mug-' + role, cls, style, t => kit(t, geo(role)).box(1.8, 1.1, 1.94, 1.24, 10, 13, ['#ecedef', '#d8dbe2', '#b4b8c0']));
function drawFolder(k) { k.box(0.84, 1.5, 1.36, 1.92, 10, 11.4, AMBER); k.top(0.92, 1.56, 1.26, 1.8, 11.4, '#ecedef'); k.top(0.98, 1.62, 1.18, 1.66, 11.45, PAL.g); }

/* Partes que mudam com o estado: tela, acessório em uso, vapor do café e, na mesa "você", cúpula da luminária e campainha. */
function liveSprites(role, v) {
  const g = geo(role), id = '-' + role + '-' + v, out = [], K = draw => t => draw(kit(t, g));
  if (role === 'approval') {
    const on = v === 'you' || v === 'done';
    const shade = on ? ['..aaaa..', '.aAAAAa.', 'aAAAAAAa', 'qqqqqqqq'] : ['..gggg..', '.gkkkkg.', 'gkkkkkkg', 'kkkkkkkk'];
    out.push(SO('lamp' + id, '', '', K(k => k.at(0.4, 1.5, 17, shade))));
    if (on) out.push(S('glow' + id, v === 'you' ? 'a-flick' : '', '', t => {
      const k = kit(t, g), p = g.pt(0.4, 1.5, 17); k.top(0.06, 1.1, 1.0, 1.9, 10.05, '#d9a44150');
      t.px(p[0] - 2, p[1], 4, 1, '#ecedef'); [1, 2, 3].forEach(n => t.px(p[0] - 3 - n, p[1] + n * 2, 1, 1, '#e3ba6a80'));
    }));
    out.push(SO('bell' + id, v === 'you' ? 'a-shake' : '', '', K(k => { k.box(1.56, 1.36, 1.8, 1.6, 10, 12, AMBER); k.box(1.64, 1.44, 1.72, 1.52, 12, 13, ['#ecedef', '#d8dbe2', '#b4b8c0']); })));
    if (v === 'you') frames(2, 0.36, n => S('ring' + n, '', '', t => {
      const p = g.pt(1.68, 1.48, 13);
      t.px(p[0] - 5 - n, p[1] - 2 - n, 1, 1, PAL.a); t.px(p[0] + 4 + n, p[1] - 2 - n, 1, 1, PAL.a);
      if (n) { t.px(p[0] - 5, p[1] + 1, 1, 1, PAL.a); t.px(p[0] + 4, p[1] + 1, 1, 1, PAL.a); }
    })).forEach(s => out.push(s));
    return out;
  }
  if (v === 'working') frames(3, role === 'tester' ? 0.9 : 0.72, n => S('code' + n + id, '', '', K(k => drawCode(k, n, role === 'tester')))).forEach(s => out.push(s));
  else if (v === 'failed') {
    out.push(S('scr' + id, '', '', K(k => drawScreen(k, '#4a1717'))));
    out.push(S('hot' + id, 'a-flick', '', K(k => drawScreen(k, '#7a2222'))));
    out.push(S('x' + id, '', '', t => { const p = g.pt(0.4, 1.5, 21); t.map(p[0] - 2, p[1], ['r...r', '.r.r.', '..r..', '.r.r.', 'r...r']); }));
  } else if (v === 'done') out.push(S('scr' + id, '', '', t => { drawScreen(kit(t, g), SCREEN); const p = g.pt(0.4, 1.5, 20); t.map(p[0] - 2, p[1], ['....N', '...N.', 'N.N..', '.N...']); }));
  else if (v === 'skipped') out.push(S('scr' + id, '', '', K(k => drawScreen(k, '#0b0c0f'))));
  else if (v === 'you') {
    out.push(S('scr' + id, '', '', K(k => { drawScreen(k, SCREEN); k.side(0.4, 1.36, 1.8, 18, 19, PAL.A); k.side(0.4, 1.5, 1.8, 16, 17, PAL.A); })));
    out.push(S('cur' + id, 'a-flick', '', K(k => k.side(0.4, 1.38, 1.46, 16, 17, PAL.a))));
  } else {
    out.push(S('scr' + id, '', '', K(k => drawScreen(k, SCREEN))));
    out.push(S('led' + id, 'a-led', '', K(k => k.side(0.4, 1.2, 1.28, 15, 16, PAL.g))));
  }
  const busy = v === 'working';
  if (role === 'triage') out.push(SO('lupa' + id, busy ? 'a-nod' : '', '', K(k => k.at(1.62, 1.74, 10.5, LUPA))));
  else if (role === 'planner') out.push(SO('pencil' + id, busy ? 'a-tapL' : '', '', K(k => k.at(1.88, 1.7, 10, ['a', 'a', 'K']))));
  else if (role === 'implementer') out.push(S('nb' + id, busy ? 'a-flick' : '', 'animation-duration:.5s', K(k => k.front(1.47, 0.62, 0.74, 12, 13, PAL.a))));
  else if (role === 'reviewer') out.push(SO('pen' + id, busy ? 'a-tapR' : '', '', K(k => k.at(1.86, 1.72, 10, ['R', 'R', 'R', 'K']))));
  else if (role === 'tester' && busy) frames(2, 0.7, n => S('bub' + n + id, '', '', K(k => k.at(1.64, 1.76, 17, n ? ['n...n', '.....', '..n..'] : ['..n..', '.....', 'n...n'])))).forEach(s => out.push(s));
  else if (role === 'code_review') out.push(SO('bug' + id, busy ? 'a-flick' : '', 'animation-duration:.8s', K(k => k.at(1.86, 1.72, 10.5, ['r.r', '.R.', 'RRR', '.R.', 'r.r']))));
  else if (role === 'reporter' && busy) frames(3, 0.9, n => S('wave' + n + id, '', '', t => { const p = g.pt(1.64, 1.76, 13); t.px(p[0] + (g.se ? 5 + n * 2 : -6 - n * 2), p[1] - 3 - n, 1, 3 + n * 2, PAL.a); })).forEach(s => out.push(s));
  if (v === 'done') frames(2, 1.1, n => S('steam' + n + id, '', '', t => { const p = g.pt(1.87, 1.17, 14); t.px(p[0] - n, p[1] - 2, 1, 1, PAL.w); t.px(p[0] - 1 + n, p[1] - 4, 1, 1, PAL.w); })).forEach(s => out.push(s));
  return out;
}
/* Móveis fixos sobre a mesa: monitor, teclado, mouse e o acessório do papel; na mesa "você", pé da luminária, papéis e vaso. */
function deskSprites(role) {
  const g = geo(role);
  return [SO('desk-' + role, '', '', t => {
    const k = kit(t, g);
    drawDesk(k);
    if (role === 'approval') {
      k.box(0.24, 1.36, 0.56, 1.64, 10, 11, STEEL); k.box(0.37, 1.47, 0.43, 1.53, 11, 17, ['#8b909a', '#8b909a', '#5a5f69']);
      k.top(1.06, 1.3, 1.5, 1.86, 10.02, '#d8dbe2'); k.top(1.12, 1.4, 1.42, 1.47, 10.03, PAL.b); k.top(1.12, 1.56, 1.34, 1.63, 10.03, PAL.g);
      k.box(1.74, 1.1, 1.92, 1.28, 10, 12.5, ['#9a6a52', '#7a5240', '#5a3c2e']); k.at(1.83, 1.19, 12.5, ['.n.', 'nNn', '.N.']);
      return;
    }
    drawMonitor(k);
    k.box(1.0, 1.1, 1.58, 1.3, 10, 11, ['#5a5f69', '#3d424d', '#2b2e36']);
    /* Teclas: grade de pixels claros com o vão escuro entre elas. */
    k.top(1.04, 1.13, 1.54, 1.27, 11, (a, b, X, Y) => ((X + (Y >> 1)) % 3 === 0 || Y % 2 ? '#4a4f59' : '#7a808b'));
    k.box(1.66, 1.14, 1.76, 1.24, 10, 11, ['#8b909a', '#5a5f69', '#3d424d']);
    if (role === 'triage') { k.top(1.4, 1.52, 1.9, 1.92, 10.02, '#d8dbe2'); k.top(1.48, 1.62, 1.8, 1.67, 10.03, PAL.g); k.top(1.48, 1.76, 1.7, 1.81, 10.03, PAL.g); }
    else if (role === 'planner') k.at(1.62, 1.74, 10, CLIP);
    else if (role === 'implementer') { k.box(0.5, 1.48, 0.98, 1.88, 10, 10.6, LIGHT); k.box(0.5, 1.42, 0.98, 1.48, 10.6, 15.5, CASE); k.front(1.48, 0.55, 0.93, 11.2, 15, SCREEN); k.front(1.48, 0.6, 0.86, 13.6, 14.4, '#e0894e'); }
    else if (role === 'reviewer') { k.top(1.4, 1.5, 1.86, 1.92, 10.02, '#ecedef'); k.top(1.48, 1.58, 1.78, 1.63, 10.03, PAL.N); k.top(1.48, 1.7, 1.78, 1.75, 10.03, PAL.R); k.top(1.48, 1.82, 1.7, 1.87, 10.03, PAL.N); }
    else if (role === 'tester') k.at(1.64, 1.76, 10, TUBES);
    else if (role === 'code_review') { k.top(1.38, 1.5, 1.64, 1.74, 10.02, PAL.A); k.top(1.5, 1.6, 1.8, 1.92, 10.03, '#ecedef'); k.top(1.56, 1.66, 1.74, 1.71, 10.04, PAL.R); k.top(1.56, 1.76, 1.7, 1.81, 10.04, PAL.N); k.top(1.56, 1.86, 1.74, 1.9, 10.04, PAL.N); }
    else if (role === 'reporter') { k.top(0.9, 1.52, 1.3, 1.9, 10.02, '#d8dbe2'); k.top(0.98, 1.6, 1.22, 1.64, 10.03, PAL.g); k.at(1.64, 1.76, 10, MEGA); }
  })];
}

/* Camadas de uma estação, na ordem de pintura: [chão e cadeira] [corpo] [mesa] [ao vivo] [mãos] [pasta] [selo] [fumaça].
   As que dependem do estado viram duas camadas no destino de uma viagem: "aguardando" até a pasta chegar e o
   estado novo a partir daí (portões g-until/g-from em --at). Quem saiu para entregar ou foi até a sua mesa não
   aparece sentado. */
function stationLayers(st, c) {
  const role = st.role, g = geo(role), you = role === 'approval', id = '-' + role;
  const v = you && st.visual === 'working' ? 'you' : st.visual;
  const layers = [], add = (sprites, cls, style) => { if (sprites.length) layers.push({ cls: cls || '', style: style || '', sprites: sprites }); };
  const at = s => '--at:' + s.toFixed(2) + 's';
  const slot = (make, gateFrom) => {
    if (c.to) { add(make('waiting'), 'g-until', at(c.arrive)); add(make(v), 'g-from', at(c.arrive)); }
    else if (gateFrom != null) add(make(v), 'g-from', at(gateFrom));
    else add(make(v));
  };
  const person = vis => (vis !== 'skipped' && !c.away ? seatedParts(role, vis, c.index) : null);
  const back = [];
  if (c.selected) back.push(S('sel' + id, 'a-sel', '', t => {
    const x0 = g.fx, y0 = g.fy, d = 0.36, gap = 0.14, m = 0.1, b = PAL.b;
    for (let s = 0.04; s < 2; s += d + gap) {
      const e = Math.min(s + d, 1.96);
      t.top(x0 + s, y0, x0 + e, y0 + m, 0, b); t.top(x0 + s, y0 + 2 - m, x0 + e, y0 + 2, 0, b);
      t.top(x0, y0 + s, x0 + m, y0 + e, 0, b); t.top(x0 + 2 - m, y0 + s, x0 + 2, y0 + e, 0, b);
    }
  }));
  /* Poça de luz no chão da estação, na cor do estado: a tela acesa de quem trabalha, a luminária da mesa "você". */
  const pool = { working: ['#7aa6ff', 0.32], you: ['#ffc861', 0.3], failed: ['#ff5d52', 0.26], returned: ['#efa49b', 0.18], done: ['#58b88a', 0.14] }[v];
  if (pool) back.push(S('pool-' + v + id, '', '', t => { const c = g.w(1, 1.1); t.glow(c[0], c[1], 0, 1.35, pool[0], pool[1]); }));
  back.push(SO('chair' + id, '', '', t => drawChair(kit(t, g), you)));
  add(back);
  if (c.to) {
    ['waiting', v].forEach((vis, n) => { const p = person(vis); if (p) add(p.body, (n ? 'g-from' : 'g-until'), at(c.arrive)); });
  } else { const p = person(v); if (p) add(p.body, p.cls); }
  add(deskSprites(role));
  slot(vis => liveSprites(role, vis));
  slot(vis => { const p = person(vis); return p ? p.hands : []; });
  if (c.away) add([mug(role, '', '')]);
  if (c.folder) add([SO('folder' + id, '', '', t => drawFolder(kit(t, g)))], c.to ? 'g-from' : '', c.to ? at(c.arrive) : '');
  const badge = vis => {
    const b = { you: 'a-bounce', done: 'a-pop', returned: '', failed: '' }[vis];
    return b == null ? [] : [SO('badge-' + vis + id, b, '', t => t.map(g.seat[0] - 5, g.seat[1] - (vis === 'done' ? 30 : 27), BADGE[vis]))];
  };
  /* Quem foi até a sua mesa leva o pedido junto: o selo não fica na mesa vazia. */
  if (!c.visiting) slot(badge, c.away ? c.td : null);
  if (v === 'failed' && !you) add([0, 0.6, 1.2].map((d, n) => S('smoke' + n + id, 'a-smoke', d ? 'animation-delay:' + d + 's' : '', t => {
    const p = g.pt(0.32, 1.5, 23); t.px(p[0] - 3 + n * 2, p[1] - 3 - (n % 2), 2 + (n % 2), 2 + (n % 2), n === 1 ? '#8b909a99' : '#8b909acc');
  })));
  return layers;
}

/* Cenário fixo, memoizado: piso, paredes, janelas e tapetes (viewBox 0 0 360 186) e os móveis soltos,
   cada um com o seu z-index pela área que ocupa no chão. */
const LEAVES = ['.....n.....', '....nN.....', '..N.nN..n..', '..NN.N.nN..', '...NNN.NN..', 'n...NNNN...', 'Nn..NNN..n.', '.NN.NNN.nN.', '..NNNNNNN..', '...NNNNN...', '....NNN....'];
const LEAF_PAL = { N: '#2f6b4f', n: '#4a9a6f', l: '#7cc896', d: '#1f4a37' };
/* Folhas com brilho na ponta de cima e sombra na base de cada tufo. */
LEAVES.forEach((ln, j) => { LEAVES[j] = ln.split('').map((ch, i) => (ch === 'n' && (j === 0 || LEAVES[j - 1][i] === '.') ? 'l' : ch === 'N' && j + 1 < LEAVES.length && LEAVES[j + 1][i] === '.' ? 'd' : ch)).join(''); });
const SKY = [[17, 19.5, '#1f2547'], [15, 17, '#2c3468'], [13, 15, '#434c96'], [11.5, 13, '#6b63a8'], [10.5, 11.5, '#b0708a'], [9.5, 10.5, '#d9a441']];
function backdrop() {
  const G = GRID, FRAME = '#3a3e48', out = [];
  /* Janela numa parede: f = face da parede (faceY para a da direita, faceX para a da esquerda), entre p0 e p1. */
  const win = (f, p0, p1) => {
    f(p0, p1, 7.5, 21.5, FRAME);
    /* Céu: as faixas do pôr do sol se misturam em pontilhado, com algumas estrelas no alto. */
    f(p0 + .12, p1 - .12, 9.5, 19.5, (u, z, X, Y) => {
      if (z > 14.5 && hash(X, Y) < .012) return '#d8dbe2';
      const zz = z + (bayer(X, Y) - .5) * 1.4, b = SKY.find(q => zz >= q[0] && zz < q[1]) || (zz >= 17 ? SKY[0] : SKY[SKY.length - 1]);
      return b[2];
    });
    /* Prédios com janelas acesas aqui e ali. */
    const bld = (X, Y) => ((X & 3) === 1 && (Y & 3) === 1 && hash(X >> 2, Y >> 2) < .3 ? '#e3ba6a' : (X & 3) === 1 && (Y & 3) === 1 && hash(X >> 2, Y >> 2) < .4 ? '#5a5f69' : '#14151a');
    [[.25, .5, 13], [.6, .8, 15.5], [.95, 1.3, 12], [1.4, 1.52, 14]].forEach(s => f(p0 + s[0], p0 + s[1], 9.5, s[2], (u, z, X, Y) => (z > s[2] - .6 ? '#23252b' : bld(X, Y))));
    f((p0 + p1) / 2 - .04, (p0 + p1) / 2 + .04, 9.5, 19.5, FRAME); f(p0 + .12, p1 - .12, 14.4, 15, FRAME);
    f(p0 - .06, p1 + .06, 6.5, 7.5, '#4a4e57');
  };
  out.push(S('bg-room', '', '', t => {
    const wallR = (x0, x1, z0, z1, c) => t.faceY(0, x0, x1, z0, z1, c), wallL = (y0, y1, z0, z1, c) => t.faceX(0, y0, y1, z0, z1, c);
    t.faceY(G, 0, G, -4, 0, '#1b1d23'); t.faceX(G, 0, G, -4, 0, '#141519');
    /* Piso: a sala escurece em degraus pontilhados para a frente (a luz vem das janelas do fundo); cada ladrilho
       tem chanfro claro nas bordas do fundo, sombra nas da frente e alguns salpicos de desgaste. */
    const far = (tx, ty, X, Y) => -0.2 * dith((tx + ty) / (2 * G), 4, X, Y);
    const r2 = v => Math.round(v * 100) / 100;
    const floor = base => (tx, ty, X, Y) => {
      const fx = fr(tx), fy = fr(ty), h = hash(X, Y);
      let d = far(tx, ty, X, Y);
      if (fx < .1 || fy < .1) d += .07; else if (fx > .9 || fy > .9) d -= .08;
      if (h < .014) d += .06; else if (h > .992) d -= .07;
      return tone(base, r2(d));
    };
    t.top(0, 0, G, G, 0, (tx, ty, X, Y) => tone('#24272f', r2(far(tx, ty, X, Y))));
    for (let x = 0; x < G; x++) for (let y = 0; y < G; y++) t.top(x + .05, y + .05, x + .95, y + .95, 0, floor((x + y) % 2 ? '#2c2f39' : '#31343f'));
    /* Tapetes: trama em xadrez fino e a sombra da sala por cima. */
    const rug = base => (tx, ty, X, Y) => tone(base, r2(far(tx, ty, X, Y) * .6 + ((X + Y) & 1 ? .035 : 0)));
    t.top(.6, 9.7, 3.6, 12.6, 0, rug('#3d447a')); t.top(.72, 9.82, 3.48, 12.48, 0, rug('#2d3254')); t.top(1, 10.1, 3.2, 12.2, 0, rug('#343a62')); t.top(1.12, 10.22, 3.08, 12.08, 0, rug('#2d3254'));
    t.top(3.7, .1, 6.3, 2.9, 0, rug('#3a3f6e')); t.top(3.82, .22, 6.18, 2.78, 0, rug('#30355c'));
    t.top(10.3, 5.2, 12.7, 8.8, 0, rug('#2f4a45')); t.top(10.42, 5.32, 12.58, 8.68, 0, rug('#27403b'));
    /* Paredes: painéis de um tile com junta, sombra pontilhada junto ao rodapé e um pouco de luz do neon no alto. */
    const wall = base => (u, z, X, Y) => {
      let d = 0;
      if (z < 7) d -= .16 * dith(1 - z / 7, 3, X, Y);
      else if (z > WALL - 9) d += .07 * dith((z - WALL + 9) / 9, 3, X, Y);
      const f = fr(u);
      if (f < 1 / 24) d -= .2; else if (f < 2 / 24) d += .05;
      return tone(base, r2(d));
    };
    wallR(0, G, 0, WALL, wall('#272a33')); wallL(0, G, 0, WALL, wall('#20232a'));
    wallR(0, G, WALL, WALL + 1, '#3d424d'); wallL(0, G, WALL, WALL + 1, '#33363e');
    wallR(0, G, 0, 2, '#1d1f25'); wallL(0, G, 0, 2, '#191b20');
    /* Fitas de neon no alto das paredes, com o brilho rebatido na parede logo abaixo. */
    wallR(.2, G - .2, WALL - 4.5, WALL - 2.5, '#4fe3e012'); wallR(.2, G - .2, WALL - 3.2, WALL - 2.5, '#4fe3e02a'); wallR(.2, G - .2, WALL - 2.5, WALL - 1.8, '#6ff0ea');
    wallL(.2, G - .2, WALL - 4.5, WALL - 2.5, '#e05fd012'); wallL(.2, G - .2, WALL - 3.2, WALL - 2.5, '#e05fd02a'); wallL(.2, G - .2, WALL - 2.5, WALL - 1.8, '#f08ae2');
    win(wallR, 4.2, 5.8); win(wallR, 6.3, 7.9); win(wallR, 10.2, 11.8); win(wallL, 6.2, 7.8); win(wallL, 10.3, 11.9);
    /* Quadro de tarefas atrás do Planejador e relógio sobre a copa. */
    wallL(4.1, 5.9, 8, 20.5, '#33363e'); wallL(4.18, 5.82, 9, 19.5, '#17181c'); wallL(4.18, 5.82, 17.5, 19.5, '#23252b');
    wallL(4.7, 4.74, 9, 19.5, '#2b2e36'); wallL(5.26, 5.3, 9, 19.5, '#2b2e36');
    [[4.25, 15, PAL.a], [4.25, 12.5, PAL.b], [4.25, 10, PAL.r], [4.8, 15, PAL.b], [5.34, 15, PAL.n], [5.34, 12.5, PAL.n], [5.34, 10, PAL.n]].forEach(n => wallL(n[0], n[0] + .3, n[1], n[1] + 2, n[2]));
    wallR(1.1, 1.9, 13, 21, '#3a3e48'); wallR(1.18, 1.82, 14, 20, '#d8dbe2'); wallR(1.46, 1.54, 16.5, 19.5, PAL.K); wallR(1.5, 1.7, 16.5, 17.3, PAL.K);
    wallL(.1, 1.9, 16, 22, '#2b2e36'); wallL(.1, 1.9, 15.4, 16, '#3a3e48');
  }));
  out.push(S('bg-light', '', '', t => {
    /* Luar das janelas no chão e as poças quentes da luminária de pé e da copa. */
    [[4.2, 5.8], [6.3, 7.9], [10.2, 11.8]].forEach(w => { t.top(w[0], .1, w[1], 2.6, 0, '#a6b0f710'); t.top(w[0] + .2, .1, w[1] - .2, 1.6, 0, '#a6b0f714'); });
    [[6.2, 7.8], [10.3, 11.9]].forEach(w => { t.top(.1, w[0], 2.6, w[1], 0, '#a6b0f70c'); t.top(.1, w[0] + .2, 1.6, w[1] - .2, 0, '#a6b0f710'); });
    t.glow(.6, 12.4, 0, 2.6, '#ffc861', .2);
    /* O neon das paredes rebate no chão, numa faixa que some em pontilhado. */
    t.top(.2, 0, GRID - .2, .9, 0, (tx, ty, X, Y) => (dith(1 - ty / .9, 3, X, Y) ? '#4fe3e0' + ['00', '0c', '14', '1e'][dith(1 - ty / .9, 3, X, Y) * 3] : '#00000000'));
    t.top(0, .2, .9, GRID - .2, 0, (tx, ty, X, Y) => (dith(1 - tx / .9, 3, X, Y) ? '#e05fd0' + ['00', '0c', '14', '1e'][dith(1 - tx / .9, 3, X, Y) * 3] : '#00000000'));
    t.glow(1.3, .9, 0, 1.6, '#ffc861', .1);
  }));
  out.push(S('bg-note', 'a-flick', 'animation-duration:1.6s', t => t.faceX(0, 4.8, 5.1, 12.5, 14.5, PAL.n)));
  [[4.5, 18, 2.6], [5.3, 17, 3.5], [10.6, 18.5, 4.3], [11.4, 16.5, 3.1]].forEach((s, n) => out.push(S('bg-star' + n, 'a-flick', 'animation-duration:' + s[2] + 's', t => t.faceY(0, s[0], s[0] + .09, s[1], s[1] + 1, PAL.b))));
  [[4.6, 1.2, 0], [5.3, 1.9, 2.2], [10.6, 1.1, 3.4], [11.3, 2, 1.1], [1.2, 6.8, 4.4], [1.8, 10.9, 5.2]].forEach((m, n) => out.push(S('bg-mote' + n, 'a-mote', 'animation-delay:-' + m[2] + 's', t => t.px(Math.round(sx(m[0], m[1])), Math.round(sy(m[0], m[1], 6)), 1, 1, PAL.b))));
  return out;
}
/* Móvel solto: tiles que ocupa (bloqueiam o caminho) e z-index logo atrás da posição mais à frente de quem passa. */
function prop(key, tiles, sprites) {
  let x0 = 1e9, y0 = 1e9, x1 = -1e9, y1 = -1e9, tx0 = 1e9, ty0 = 1e9, tx1 = -1e9, ty1 = -1e9;
  sprites.forEach(s => { const b = s.art.box; x0 = Math.min(x0, b[0]); y0 = Math.min(y0, b[1]); x1 = Math.max(x1, b[0] + b[2]); y1 = Math.max(y1, b[1] + b[3]); });
  tiles.forEach(p => { tx0 = Math.min(tx0, p[0]); ty0 = Math.min(ty0, p[1]); tx1 = Math.max(tx1, p[0] + 1); ty1 = Math.max(ty1, p[1] + 1); });
  const z = Math.round((Math.min(tx0 + ty1, tx1 + ty0) + 0.5) * 10);
  return { key: key, tiles: tiles, vb: x0 + ' ' + y0 + ' ' + (x1 - x0) + ' ' + (y1 - y0), style: '--x:' + x0 + ';--y:' + y0 + ';--w:' + (x1 - x0) + ';--h:' + (y1 - y0) + ';z-index:' + z, sprites: sprites };
}
function plant(key, x, y) {
  return prop(key, [[x, y]], [
    SO(key, '', '', t => { t.box(x + .28, y + .28, x + .72, y + .72, 0, 6, ['#9a6a52', '#7a5240', '#5a3c2e']); t.at(x + .5, y + .5, 6, LEAVES.slice(5), LEAF_PAL); }),
    SO(key + '-top', 'a-sway', '', t => t.at(x + .5, y + .5, 12, LEAVES.slice(0, 5), LEAF_PAL))
  ]);
}
function props() {
  return memo('props@' + TAG, () => {
    const SOFA = ['#5b8f99', '#3f6b73', '#335961'], CAB = ['#3d424d', '#33363e', '#2b2e36'], CUP = ['#ecedef', '#d8dbe2', '#b4b8c0'];
    const pxAt = (t, x, y, z, dx, dy, w, h, c) => t.px(Math.round(sx(x, y)) + dx, Math.round(sy(x, y, z)) + dy, w, h, c);
    return [
      /* Copa no canto do fundo: bancada em L com cafeteira, pia e geladeira. */
      prop('copa-r', [[0, 0], [1, 0]], [
        SO('copa-r', '', '', t => {
          t.box(.06, .06, 1.94, .86, 0, 9, CAB); t.box(.02, .02, 1.98, .92, 9, 10.5, LIGHT);
          t.faceY(.86, .98, 1.02, 1, 8, '#1f2127');
          t.box(1.2, .14, 1.74, .62, 10.5, 19, ['#d9645a', '#b84a42', '#8f3832']); t.faceY(.62, 1.32, 1.62, 12, 15.5, '#17181c'); t.box(1.4, .62, 1.54, .74, 10.5, 13, CUP);
          t.box(.3, .2, .42, .34, 10.5, 13, CUP); t.box(.52, .3, .64, .44, 10.5, 13, ['#a6b0f7', '#7b88e6', '#5b6ad9']);
          t.faceY(0, .1, 1.9, 16, 22, '#2b2e36'); t.faceY(0, .1, 1.9, 15.4, 16, '#3a3e48');
        }),
        S('copa-led', 'a-led', '', t => t.faceY(.62, 1.66, 1.7, 13, 14, PAL.n))
      ].concat(frames(2, 1.2, n => S('copa-steam' + n, '', '', t => { pxAt(t, 1.47, .68, 13, -n, -2, 1, 1, PAL.w); pxAt(t, 1.47, .68, 13, n - 1, -4, 1, 1, PAL.w); })))),
      prop('copa-l', [[0, 1]], [SO('copa-l', '', '', t => {
        t.box(.06, 1.0, .86, 1.94, 0, 9, CAB); t.box(.02, .92, .92, 1.98, 9, 10.5, LIGHT);
        t.top(.2, 1.2, .7, 1.7, 10.5, '#8b909a'); t.top(.26, 1.26, .64, 1.64, 10.5, '#5a5f69'); t.box(.12, 1.42, .2, 1.5, 10.5, 15, STEEL);
      })]),
      prop('fridge', [[2, 0]], [SO('fridge', '', '', t => { t.box(2.1, .08, 2.9, .86, 0, 25, ['#d8dbe2', '#b4b8c0', '#9aa0ab']); t.faceY(.86, 2.1, 2.9, 14, 14.8, '#6c717c'); t.faceY(.86, 2.74, 2.82, 7, 12, '#5a5f69'); t.faceY(.86, 2.74, 2.82, 17, 22, '#5a5f69'); })]),
      plant('plant-a', 0, 2),
      prop('shelf', [[8, 0], [9, 0]], [SO('shelf', '', '', t => {
        t.box(8.05, .05, 9.95, .5, 0, 22, WOOD_D);
        [1, 8, 15].forEach((z, row) => { t.faceY(.5, 8.12, 9.88, z, z + 6, '#2a2019'); ['#6f7de0', '#dd6a83', '#49b5b0', '#e3ba6a', '#8fc45a', '#b377d9', '#d8dbe2', '#e0894e'].forEach((c, n) => { const x = 8.16 + n * .2 + (row % 2) * .08; if (x < 9.7 && (n + row) % 4 !== 3) t.faceY(.5, x, x + .16, z, z + 4 + (n * 7 + row) % 2, c); }); });
      })]),
      prop('cooler', [[10, 0]], [
        SO('cooler', '', '', t => { t.box(10.22, .2, 10.78, .72, 0, 12, LIGHT); t.box(10.3, .26, 10.7, .66, 12, 21, ['#7fb6d6', '#4f86a8', '#3f6f8f']); t.faceY(.72, 10.36, 10.46, 7, 9, PAL.R); t.faceY(.72, 10.54, 10.64, 7, 9, PAL.B); })
      ].concat(frames(3, 2.1, n => S('cooler-bub' + n, '', '', t => pxAt(t, 10.5, .66, 14 + n * 2, n - 1, 0, 1, 1, '#cfe6f5'))))),
      prop('printer', [[11, 0]], [
        SO('printer', '', '', t => { t.box(11.1, .1, 11.9, .9, 0, 8, CAB); t.box(11.14, .14, 11.86, .86, 8, 14, LIGHT); t.faceY(.86, 11.24, 11.76, 9.5, 11, '#17181c'); t.top(11.3, .3, 11.7, .6, 14, '#ecedef'); }),
        S('printer-paper', 'a-paper', '', t => t.top(11.3, .86, 11.7, 1.06, 10, '#ecedef')),
        S('printer-led', 'a-flick', 'animation-duration:1.4s', t => t.faceY(.86, 11.78, 11.84, 12, 13, PAL.n))
      ]),
      plant('plant-c', 12, 0),
      /* Área de descanso na frente, à esquerda: sofá, mesa de centro e luminária. */
      prop('sofa', [[0, 10], [0, 11]], [SO('sofa', '', '', t => {
        t.box(.1, 10.1, .36, 11.9, 0, 13, SOFA); t.box(.1, 10.1, .96, 10.32, 0, 9, SOFA);
        t.box(.36, 10.32, .96, 11.68, 0, 5, ['#6aa3ae', SOFA[1], SOFA[2]]); t.top(.4, 10.98, .94, 11.02, 5, SOFA[1]);
        t.box(.44, 10.42, .6, 10.74, 5, 8, AMBER);
        t.box(.1, 11.68, .96, 11.9, 0, 9, SOFA);
      })]),
      prop('table', [[2, 10], [2, 11]], [SO('table', '', '', t => { t.box(2.2, 10.65, 2.8, 11.35, 0, 3, WOOD_D); t.box(2.1, 10.55, 2.9, 11.45, 3, 5, WOOD); t.top(2.3, 10.75, 2.62, 11.1, 5, '#d8dbe2'); t.top(2.34, 10.79, 2.58, 10.87, 5, PAL.r); t.box(2.66, 11.16, 2.78, 11.3, 5, 7.5, CUP); })]),
      prop('lamp', [[0, 12]], [
        SO('lamp', '', '', t => { t.box(.32, 12.32, .68, 12.68, 0, 1.5, STEEL); t.box(.46, 12.46, .54, 12.54, 1.5, 21, ['#8b909a', '#8b909a', '#5a5f69']); t.box(.26, 12.26, .74, 12.74, 21, 27, AMBER); }),
        S('lamp-glow', '', '', t => t.glow(.5, 12.5, 24, .7, '#ffd27a', .22))
      ]),
      /* Canto de reunião à direita: mesa redonda com bancos, sobre o tapete verde. */
      prop('meet', [[11, 6], [11, 7]], [SO('meet', '', '', t => {
        t.box(11.44, 6.82, 11.56, 6.94, 0, 9, STEEL); t.box(11.06, 6.44, 11.94, 7.32, 9, 10.5, WOOD);
        t.top(11.2, 6.62, 11.5, 6.86, 10.5, '#ecedef'); t.box(11.62, 6.96, 11.74, 7.1, 10.5, 13, CUP);
      })]),
      prop('stool-a', [[11, 5]], [SO('stool-a', '', '', t => { t.box(11.45, 5.45, 11.55, 5.55, 0, 5, STEEL); t.box(11.26, 5.26, 11.74, 5.74, 5, 6.5, ['#dd6a83', '#9c4257', '#7a3245']); })]),
      prop('stool-b', [[11, 8]], [SO('stool-b', '', '', t => { t.box(11.45, 8.45, 11.55, 8.55, 0, 5, STEEL); t.box(11.26, 8.26, 11.74, 8.74, 5, 6.5, ['#49b5b0', '#2c7f7b', '#22625f']); })]),
      plant('plant-d', 12, 11),
      plant('plant-e', 9, 12)
    ];
  });
}
/* O cenário inteiro, como sai em buildView: fundo (um SVG) e móveis (um elemento cada). */
function scenery() { return memo('scenery@' + TAG, () => ({ backdrop: backdrop(), props: props() })); }

/* Tiles onde ninguém anda: estações e móveis. O resto é corredor. */
function blocked() {
  return memo('blocked', () => {
    const b = {};
    ORDER.forEach(r => { const s = STATIONS[r].at; [[0, 0], [1, 0], [0, 1], [1, 1]].forEach(d => { b[(s[0] + d[0]) + ',' + (s[1] + d[1])] = 1; }); });
    props().forEach(p => p.tiles.forEach(q => { b[q[0] + ',' + q[1]] = 1; }));
    return b;
  });
}
/* Caminho pelos corredores entre dois tiles: menor custo, 1 por tile e 0,6 por curva (prefere trechos retos). */
function route(from, to) {
  return memo('route:' + from + '>' + to, () => {
    const B = blocked(), DIRS = [[1, 0], [0, 1], [-1, 0], [0, -1]], best = {}, queue = [{ x: from[0], y: from[1], d: -1, cost: 0, prev: null }];
    while (queue.length) {
      queue.sort((p, q) => p.cost - q.cost);
      const cur = queue.shift();
      if (cur.x === to[0] && cur.y === to[1]) { const path = []; for (let n = cur; n; n = n.prev) path.unshift([n.x, n.y]); return path; }
      DIRS.forEach((dir, d) => {
        const x = cur.x + dir[0], y = cur.y + dir[1];
        if (x < 0 || y < 0 || x >= GRID || y >= GRID || B[x + ',' + y]) return;
        const cost = cur.cost + 1 + (cur.d >= 0 && cur.d !== d ? 0.6 : 0), id = x + ',' + y + ',' + d;
        if (best[id] != null && best[id] <= cost) return;
        best[id] = cost; queue.push({ x: x, y: y, d: d, cost: cost, prev: cur });
      });
    }
    return null;
  });
}

/* Viagem da pasta, derivada só de { active, previous }: { kind, td, arrive, points, legs }.
   A pessoa da mesa de origem levanta (fica de pé atrás da mesa), sai pelo lado de caminho mais curto, anda pelos
   corredores com a pasta, entrega na ponta da mesa seguinte, ao lado da cadeira, e volta. Da mesa "você" a pessoa
   não levanta: a pasta voa como aviãozinho de papel. Cada trecho reto é um elemento com posição e z-index animados
   em passos de 2x1 px; fora da sua janela de tempo ele fica com opacity 0. */
function legStyle(a, b, za, zb, t0, dur, steps) {
  return '--x0:' + a[0] + ';--y0:' + a[1] + ';--x1:' + b[0] + ';--y1:' + b[1] + ';--z0:' + za + ';--z1:' + zb +
    ';--t0:' + t0.toFixed(2) + 's;--dur:' + dur.toFixed(2) + 's;--ease:steps(' + steps + ',end)';
}
/* Canto superior esquerdo da caixa 12x20 de quem anda, com os pés no ponto do chão p. */
const feet = p => [sx(p[0], p[1]) - 6, sy(p[0], p[1]) - 20];
/* Rota a pé da mesa de origem até a ponta da mesa de destino, só com os cantos: a pessoa levanta (de pé atrás da
   mesa), sai pelo lado de caminho mais curto e anda pelos corredores. null se não houver caminho. */
function walkPath(from, to) {
  return memo('path:' + from + '>' + to, () => {
    const B = blocked(), gd = geo(to), goal = [Math.floor(gd.front[0]), Math.floor(gd.front[1])];
    let best = null;
    geo(from).exits.forEach(e => {
      const out = [Math.floor(e.out[0]), Math.floor(e.out[1])];
      if (out[0] < 0 || out[1] < 0 || B[out[0] + ',' + out[1]]) return;
      const r = route(out, goal); if (!r) return;
      const pts = [e.stand].concat(r.map(q => [q[0] + .5, q[1] + .5]));
      if (!best || pts.length < best.length) best = pts;
    });
    return best && best.filter((q, i) => {
      if (i === 0 || i === best.length - 1) return true;
      const a = best[i - 1], b = best[i + 1];
      return (q[0] - a[0]) * (b[1] - q[1]) !== (q[1] - a[1]) * (b[0] - q[0]);
    });
  });
}
/* Trechos de quem anda, um depois do outro: andando (ciclo de 4 quadros, um passo de 2x1 px a cada 1/6 de tile) ou
   parado (entregando a pasta, ou de pé esperando com hold, que fica à vista depois do fim). bare: sem os sprites. */
function walker(role, bare) {
  const w = { legs: [], t: 0 };
  const face = (p, q) => (q[0] > p[0] ? 'se' : q[0] < p[0] ? 'nw' : q[1] > p[1] ? 'sw' : 'ne');
  w.leg = (p, q, dir, carry, hold) => {
    const len = Math.abs(q[0] - p[0]) + Math.abs(q[1] - p[1]), dur = len ? len / SPEED : HAND;
    w.legs.push({ cls: 'leg f-' + dir + (hold ? ' hold' : ''), vb: '0 0 12 20', style: legStyle(feet(p), feet(q), depth(p), depth(q), w.t, dur, Math.max(1, Math.round(len * 6))), sprites: bare ? [] : walkerSprites(role, dir === 'ne' || dir === 'nw', carry, len ? null : 0) });
    w.t += dur;
  };
  w.go = (pts, carry) => { for (let i = 0; i + 1 < pts.length; i++) w.leg(pts[i], pts[i + 1], face(pts[i], pts[i + 1]), carry); };
  w.back = pts => { for (let i = pts.length - 1; i > 0; i--) w.leg(pts[i], pts[i - 1], face(pts[i], pts[i - 1]), false); };
  return w;
}
/* bare: só a rota e os tempos, sem desenhar os sprites (quem só quer saber quanto a viagem dura). */
function tripOf(scene, bare) {
  const ai = scene.active, pi = scene.previous;
  if (ai == null || pi == null || ai === pi || !scene.stations[ai] || !scene.stations[pi]) return null;
  const from = scene.stations[pi].role, go = geo(from), gd = geo(scene.stations[ai].role);
  const base = { id: scene.tripId || 0, fromIndex: pi, toIndex: ai };
  if (from === 'approval') {
    const p = go.w(1, 1.5), q = gd.w(1.1, 1.7), td = 1.8;
    const a = [sx(p[0], p[1]) - 6, sy(p[0], p[1], 16) - 6], b = [sx(q[0], q[1]) - 6, sy(q[0], q[1], 13) - 6];
    return Object.assign(base, { kind: 'plane', td: td, arrive: td, points: [p, q], legs: [{
      cls: 'leg plane f-' + (b[0] < a[0] ? 'sw' : 'se'), vb: '0 0 12 6',
      style: legStyle(a, b, 2900, 2900, 0, td, 36), sprites: bare ? [] : [SO('plane', '', '', t => t.map(0, 0, PLANE))]
    }] });
  }
  const corners = walkPath(from, scene.stations[ai].role);
  if (!corners) return null;
  const w = walker(from, bare), end = corners[corners.length - 1];
  w.go(corners, true);
  const arrive = w.t;
  w.leg(end, end, gd.facing, false);
  w.back(corners);
  return Object.assign(base, { kind: 'walk', td: +w.t.toFixed(2), arrive: +arrive.toFixed(2), points: corners, legs: w.legs });
}
/* Visita à sua mesa, derivada só de scene.visit { index, phase, id }: quem pede a sua autorização no meio da etapa
   (uma pergunta, o plano, uma permissão) levanta sem a pasta, anda até a ponta da sua mesa e fica de pé, virado
   para ela, até você responder ('go'; 'stay' já começa lá, para quem abre a execução no meio do pedido).
   Respondido, volta para a mesa ('back'); td é quando a ida chega ou a volta acaba. */
function visitOf(scene, bare) {
  const v = scene.visit, st = v && scene.stations[v.index];
  if (!st || st.role === 'approval' || !scene.stations.some(s => s.role === 'approval')) return null;
  const corners = walkPath(st.role, 'approval');
  if (!corners) return null;
  const w = walker(st.role, bare), end = corners[corners.length - 1], facing = geo('approval').facing;
  if (v.phase === 'go') w.go(corners, false);
  if (v.phase === 'back') { w.leg(end, end, facing, false); w.back(corners); }
  else w.leg(end, end, facing, false, true);
  return { id: v.id || 0, kind: 'visit', fromIndex: v.index, phase: v.phase, td: +w.t.toFixed(2), legs: w.legs };
}

/* Estado da cena -> tudo o que a marcação precisa. opt: { scale, dpr, traveling }.
   traveling = true enquanto a viagem atual anda; quem monta o componente desliga depois de trip.td segundos. */
function buildView(scene, opt) {
  opt = opt || {};
  density(opt.scale || 3, opt.dpr || 1);
  const trip = opt.traveling ? tripOf(scene) : null, visit = visitOf(scene);
  const stations = scene.stations.map((st, i) => {
    const g = geo(st.role), selected = scene.selected === i, active = scene.active === i;
    const from = !!trip && trip.fromIndex === i, to = !!trip && trip.toIndex === i, visiting = !!visit && visit.fromIndex === i;
    const layers = stationLayers(st, {
      index: i, selected: selected, folder: active && st.visual !== 'skipped',
      to: to, arrive: trip ? trip.arrive : 0, away: (from && trip.kind === 'walk') || visiting, td: trip ? trip.td : 0, visiting: visiting
    });
    const meta = [st.account, st.model, st.time].filter(Boolean).join(' · ');
    const working = st.visual === 'working' && st.role !== 'approval';
    let aria = st.label + ': ' + (st.stateText || st.visual);
    if (st.pass) aria += ', ' + st.pass;
    if (working && st.activity) aria += '. ' + st.activity;
    if (working && meta) aria += '. ' + meta;
    const b = g.box, ax = g.seat[0];
    return {
      role: st.role, index: i, cls: 'st v-' + st.visual + (active ? ' is-active' : '') + (selected ? ' is-selected' : ''),
      style: '--x:' + b[0] + ';--y:' + b[1] + ';--w:' + b[2] + ';--h:' + b[3] + ';--z:' + g.z + ';--lx:' + g.tag[0] + ';--ly:' + (g.tag[1] + 1),
      vb: b.join(' '), aria: aria, pressed: selected, layers: layers, label: st.label, passText: st.pass || '',
      balloon: working && st.activity ? {
        cls: 'balloon al-' + (ax < 120 ? 'l' : ax > 240 ? 'r' : 'c') + (to ? ' g-from' : ''),
        style: '--bx:' + ax + ';--by:' + (g.seat[1] - 18) + (to ? ';--at:' + trip.arrive.toFixed(2) + 's' : ''),
        activity: st.activity, meta: meta
      } : null
    };
  });
  return { stations: stations, trip: trip, visit: visit };
}

// ---- interface pública

export type OfficeVisual = "waiting" | "working" | "done" | "returned" | "failed" | "skipped" | "you";

export interface OfficeStation {
  role: string;
  visual: OfficeVisual;
  label: string;
  /** Situação em texto ("Trabalhando", "Devolveu"...), para leitores de tela. */
  stateText: string;
  activity: string | null;
  account: string | null;
  model: string | null;
  time: string;
  /** Passagem atual ("volta 2/3", "plano v2"), ou null. */
  pass: string | null;
}

export interface OfficeScene {
  stations: OfficeStation[];
  /** Mesa com a pasta (a etapa mais recente) e a de onde ela veio; índices na ordem da esteira. */
  active: number | null;
  previous: number | null;
  selected: number | null;
  /** Cresce a cada passagem da pasta, para reiniciar a viagem. */
  tripId: number;
  /** Quem foi até a sua mesa pedir autorização: ida ('go'), já lá ('stay') ou volta ('back'). `id` reinicia a cena. */
  visit: { index: number; phase: "go" | "stay" | "back"; id: number } | null;
}

interface Art { href: string; x: number; y: number; w: number; h: number; box: number[]; mx: number; my: number; mw: number; mask: Uint8Array }
interface Sprite { k: string; cls: string; style: string; art: Art }
interface Layer { cls: string; style: string; sprites: Sprite[] }

export interface OfficeTrip { id: number; kind: "walk" | "plane" | "visit"; td: number; legs: { cls: string; style: string; vb: string; sprites: Sprite[] }[] }

export interface OfficeView {
  stations: {
    role: string;
    index: number;
    cls: string;
    style: string;
    vb: string;
    aria: string;
    pressed: boolean;
    layers: Layer[];
    label: string;
    passText: string;
    balloon: { cls: string; style: string; activity: string; meta: string } | null;
  }[];
  trip: OfficeTrip | null;
  /** Ida, espera de pé ou volta de quem foi até a sua mesa (à vista enquanto `scene.visit` existir). */
  visit: OfficeTrip | null;
}

/** Largura e altura lógicas da sala. */
export const OFFICE_W: number = LW;
export const OFFICE_H: number = LH;
/** `traveling`: verdadeiro enquanto a viagem atual anda; quem monta desliga depois de `trip.td` segundos.
    `dpr`: window.devicePixelRatio (os bitmaps saem na densidade da tela; ver `density`). */
export const officeView: (scene: OfficeScene, opt: { scale: number; dpr: number; traveling: boolean }) => OfficeView = buildView;
/** Rota e duração da viagem atual, sem os sprites (`legs[].sprites` vazio). */
export const officeTrip = (scene: OfficeScene): OfficeTrip | null => tripOf(scene, true);
/** Rota e duração da visita atual à sua mesa, sem os sprites. */
export const officeVisit = (scene: OfficeScene): OfficeTrip | null => visitOf(scene, true);

const imageSvg = (a: Art): string => (a.href ? `<image href="${a.href}" x="${a.x}" y="${a.y}" width="${a.w}" height="${a.h}"/>` : "");
const spritesSvg = (list: Sprite[]): string => list.map((s) => `<g class="${s.cls}" style="${s.style}">${imageSvg(s.art)}</g>`).join("");

/* Máscara de clique: um <path> transparente com as células de px lógico pintadas pelos sprites da mesa
   (uma <image> recebe clique no retângulo todo, e as mesas vizinhas se sobrepõem). Só conta o que fica à vista:
   fora os quadros de animação além do primeiro, a fumaça, o gole e o estado de antes da chegada da pasta (g-until).
   Memoizada pelo conjunto de sprites. */
const HIDDEN = /(^|\s)(fr(?!0|\d)|a-smoke|a-sipB)(\s|$)/;
const HITS = new Map<string, string>();
function hitPath(all: Layer[]): string {
  const layers = all
    .filter((l) => !/(^|\s)g-until(\s|$)/.test(l.cls))
    .map((l) => ({ ...l, sprites: l.sprites.filter((s) => !HIDDEN.test(s.cls) || /(^|\s)fr0(\s|$)/.test(s.cls)) }));
  const key = layers.map((l) => l.sprites.map((s) => s.k).join(",")).join("|");
  let out = HITS.get(key);
  if (out == null) {
    const rows = new Map<number, Set<number>>();
    for (const l of layers) for (const s of l.sprites) {
      const a = s.art;
      for (let i = 0; i < a.mask.length; i++) {
        if (!a.mask[i]) continue;
        const y = a.my + Math.floor(i / a.mw);
        let row = rows.get(y);
        if (!row) rows.set(y, (row = new Set()));
        row.add(a.mx + (i % a.mw));
      }
    }
    let d = "";
    for (const [y, row] of rows) {
      const xs = [...row].sort((a, b) => a - b);
      for (let i = 0; i < xs.length; ) {
        let w = 1;
        while (xs[i + w] === xs[i] + w) w++;
        d += `M${xs[i]} ${y}h${w}v1h-${w}z`;
        i += w;
      }
    }
    out = d ? `<path class="hit" d="${d}" fill="transparent"/>` : "";
    HITS.set(key, out);
  }
  return out;
}

/** Camadas de uma estação como SVG, para {@html} dentro de um <svg>; por cima, a máscara de clique. */
export function layersSvg(layers: Layer[]): string {
  return layers.map((l) => `<g class="${l.cls}" style="${l.style}">${spritesSvg(l.sprites)}</g>`).join("") + hitPath(layers);
}
/** Sprites soltos (trechos da viagem) como SVG. */
export { spritesSvg };

/** Cenário fixo da sala: o fundo como SVG e os móveis (cada um com a sua profundidade). Gerado uma vez só.
    O que anima no fundo (estrelas, poeira, post-it) sai do SVG grande e vira um elemento pequeno logo acima dele:
    assim cada piscada repinta alguns pixels, e não a sala inteira. */
const sceneryCache = new Map<string, { backdrop: string; props: { style: string; vb: string; svg: string }[] }>();
export function officeScenery(scale: number, dpr: number) {
  density(scale, dpr);
  let cached = sceneryCache.get(TAG);
  if (!cached) {
    const s = scenery();
    const moving = s.backdrop.filter((sp: Sprite) => sp.cls).map((sp: Sprite) => {
      const b = sp.art.box;
      return { style: `--x:${b[0]};--y:${b[1]};--w:${b[2]};--h:${b[3]};z-index:1`, vb: b.join(" "), svg: spritesSvg([sp]) };
    });
    cached = {
      backdrop: spritesSvg(s.backdrop.filter((sp: Sprite) => !sp.cls)),
      props: moving.concat(s.props.map((p) => ({ style: p.style, vb: p.vb, svg: spritesSvg(p.sprites) }))),
    };
    sceneryCache.set(TAG, cached);
  }
  return cached;
}
