// @ts-nocheck
/* Cena do escritório isométrico da esteira de bots, vinda do protótipo no Claude Design (v2).
   Grade lógica 360x186 px escalada por inteiro (--s), desenhada por dentro com 2x2 pixels finos por px lógico
   (contorno, luz de borda, sombras e poças de luz). Sprites são mapas de caracteres ampliados pelo Scale2x e
   convertidos em um <path> de retângulos por cor; a animação fica toda no CSS (steps()) de office.css.
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
const WOOD = ['#8f7050', '#6f553c', '#5a4431'], WOOD_D = ['#6f553c', '#5a4330', '#48362a'];
const STEEL = ['#3d424d', '#2b2e36', '#1f2127'], LIGHT = ['#d8dbe2', '#b4b8c0', '#8b909a'], AMBER = ['#e3ba6a', '#d9a441', '#9c7226'];
const CASE = ['#3d424d', '#2b2e36', '#1f2127'], SCREEN = '#12151d';

/* Pintura: grade esparsa de pixels -> um <path> de retângulos por cor (a ordem de pintura se resolve na grade).
   A grade interna tem R pixels finos por px lógico: as coordenadas da cena seguem em px lógicos, mas faces,
   bordas e contornos saem com o dobro da definição, e os mapas de caracteres passam pelo Scale2x. */
const R = 2;
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
  const up = R === 2 ? scale2x(src) : src, x0 = Math.round(x * R), y0 = Math.round(y * R);
  for (let j = 0; j < up.length; j++) for (let i = 0; i < up[j].length; i++) {
    const ch = up[j][i]; if (ch === '.') continue;
    const c = (pal && pal[ch]) || PAL[ch]; if (c) paint(g, x0 + i, y0 + j, 1, 1, c);
  }
}
/* Contorno seletivo: cada pixel vazio encostado no desenho ganha a cor vizinha bem escurecida (sombras e
   brilhos translúcidos não contam). Separa os objetos uns dos outros, como na pixel art desenhada à mão. */
function outline(g) {
  const add = [];
  Object.keys(g).forEach(ys => {
    const y = +ys, row = g[y];
    Object.keys(row).forEach(xs => {
      const x = +xs, c = row[x]; if (c.length > 7) return;
      [[1, 0], [-1, 0], [0, 1], [0, -1]].forEach(d => { const nx = x + d[0], ny = y + d[1]; if (!(g[ny] && g[ny][nx])) add.push([nx, ny, shade(c, 0.72)]); });
    });
  });
  add.forEach(a => { if (!(g[a[1]] && g[a[1]][a[0]])) paint(g, a[0], a[1], 1, 1, a[2]); });
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
    if (!pos || !neg) paint(g, x, y, 1, 1, c);
  }
}
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
  const top = (x0, y0, x1, y1, z, c) => quad([[sx(x0, y0), sy(x0, y0, z)], [sx(x1, y0), sy(x1, y0, z)], [sx(x1, y1), sy(x1, y1, z)], [sx(x0, y1), sy(x0, y1, z)]], c);
  const faceY = (y, x0, x1, z0, z1, c) => quad([[sx(x0, y), sy(x0, y, z1)], [sx(x1, y), sy(x1, y, z1)], [sx(x1, y), sy(x1, y, z0)], [sx(x0, y), sy(x0, y, z0)]], c);
  const faceX = (x, y0, y1, z0, z1, c) => quad([[sx(x, y1), sy(x, y1, z1)], [sx(x, y0), sy(x, y0, z1)], [sx(x, y0), sy(x, y0, z0)], [sx(x, y1), sy(x, y1, z0)]], c);
  /* Luz de borda: a fileira de pixels finos do tampo junto às arestas da frente fica mais clara, e a quina
     vertical entre as duas faces ganha um fio de luz. Só repinta pixels que ainda têm a cor da face. */
  const rim = (a, b, c, to) => {
    const ax = a[0] * R, ay = a[1] * R, bx = b[0] * R, by = b[1] * R;
    for (let x = Math.floor(Math.min(ax, bx)); x < Math.ceil(Math.max(ax, bx)); x++) {
      const t = (x + 0.5 - ax) / (bx - ax || 1), ye = ay + (by - ay) * t, y = Math.ceil(ye - 0.5) - 1, row = g[y];
      if (row && row[x] === c) row[x] = to;
    }
  };
  const box = (x0, y0, x1, y1, z0, z1, c) => {
    /* Sombra de contato no chão, puxada para a frente (a luz vem do fundo da sala). */
    if (z0 === 0 && x1 - x0 >= 0.3 && y1 - y0 >= 0.3) top(x0 + 0.04, y0 + 0.04, x1 + 0.14, y1 + 0.14, 0, '#05060a5c');
    faceY(y1, x0, x1, z0, z1, c[1]); faceX(x1, y0, y1, z0, z1, c[2]); top(x0, y0, x1, y1, z1, c[0]);
    if (c[0].length > 7) return;
    const P = [sx(x0, y1), sy(x0, y1, z1)], Q = [sx(x1, y1), sy(x1, y1, z1)], T = [sx(x1, y0), sy(x1, y0, z1)], hi = lit(c[0], 0.38);
    rim(P, Q, c[0], hi); rim(Q, T, c[0], hi);
    const qx = Math.round(Q[0] * R) - 1, s1 = lit(c[1], 0.16);
    for (let y = Math.round(Q[1] * R); y < Math.round(sy(x1, y1, z0) * R); y++) if (g[y] && g[y][qx] === c[1]) g[y][qx] = s1;
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
  const px = (x, y, w, h, c) => paint(g, Math.round(x * R), Math.round(y * R), Math.round(w * R), Math.round(h * R), c);
  return { top: top, faceY: faceY, faceX: faceX, box: box, glow: glow, at: at, map: (x, y, lines, pal, flip) => stamp(g, x, y, lines, pal, flip), px: px };
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
function toPaths(g) {
  const rects = {}, order = [], open = {};
  let bx0 = 1e9, bx1 = -1e9, by0 = 1e9, by1 = -1e9;
  Object.keys(g).map(Number).sort((a, b) => a - b).forEach(y => {
    const row = g[y], xs = Object.keys(row).map(Number).sort((a, b) => a - b);
    by0 = Math.min(by0, y); by1 = Math.max(by1, y + 1); bx0 = Math.min(bx0, xs[0]); bx1 = Math.max(bx1, xs[xs.length - 1] + 1);
    for (let i = 0; i < xs.length;) {
      const x = xs[i], c = row[x]; let w = 1;
      while (i + w < xs.length && xs[i + w] === x + w && row[x + w] === c) w++;
      const key = c + '|' + x + '|' + w, o = open[key];
      if (o && o.y + o.h === y) o.h++;
      else { const r = { x: x, y: y, w: w, h: 1 }; open[key] = r; if (!rects[c]) { rects[c] = []; order.push(c); } rects[c].push(r); }
      i += w;
    }
  });
  /* De volta a px lógicos: meio px lógico é um pixel fino. */
  const u = n => +(n / R).toFixed(2);
  const paths = order.map(c => ({ f: c, d: rects[c].map(r => 'M' + u(r.x) + ' ' + u(r.y) + 'h' + u(r.w) + 'v' + u(r.h) + 'h-' + u(r.w) + 'z').join('') }));
  paths.box = [u(bx0), u(by0), u(bx1 - bx0), u(by1 - by0)];
  return paths;
}
/* Sprite = um <g>: classes de animação, estilo inline e os paths (memoizados pela chave do desenho).
   SO é o mesmo sprite com contorno: móveis, pessoas e objetos soltos (telas, brilhos e fumaça ficam sem). */
function S(key, cls, style, draw, ol) { return { k: key, cls: cls || '', style: style || '', paths: memo('art:' + key, () => { const g = {}; draw(iso(g)); if (ol) outline(g); return toPaths(g); }) }; }
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

/* Pessoas: uma por papel, com a roupa na cor do papel. Cabeças em 3/4 (10x8, a última linha é o pescoço), viradas
   para a esquerda: front olha para sudoeste, back para noroeste; o espelho dá sudeste e nordeste. Olhos na linha 4. */
const PEOPLE = {
  triage: { pal: { S: '#8d5a3b', s: '#6f4329', H: '#1c1a1e', h: '#3a3540', C: '#49b5b0', c: '#2c7f7b', P: '#3d4560' },
    front: ['..HHHHHH..', '.HHhHHHHH.', 'HHHHHHHHHH', 'HHSSSSHHHH', '.SSSSSSSH.', '.SSSSSSsH.', '..SSSss...', '...sSS....'],
    back: ['..HHHHHH..', '.HHHHhHHH.', 'HHHHHHHHHH', 'HHHHHHHHHH', 'sHHHHHHHHH', '.sHHHHHHH.', '..SSSSSS..', '...SSSS...'] },
  planner: { pal: { S: '#f1c9a5', s: '#d9a982', H: '#b5532f', h: '#d0703f', C: '#6f7de0', c: '#4450a8', P: '#6b5a4a' },
    front: ['..HHHHHH..', '.HHhhHHHH.', 'HHHHHHHHHH', 'HSSSSSHHHH', 'HSSSSSSHHH', 'HSSSSSSHHH', '.HSSSsHHHH', '...sSSHHH.'],
    back: ['..HHHHHH..', '.HHHhHHHH.', 'HHHHHHHHHH', 'HHHHHHHHHH', 'SHHHHHHHHH', 'sHHHHHHHHH', '.HHHHHHHHH', '..HHHHHHH.'] },
  implementer: { pal: { S: '#c68c5f', s: '#a8714a', H: '#2b2320', h: '#4a3a33', C: '#e0894e', c: '#a85f30', P: '#55607d', G: '#b4b8c0', g: '#5a5f69' },
    front: ['..gggggg..', '.gHHHHHHg.', '.HHHhHHHGG', 'HHSSSSHGGG', '.SSSSSSGGG', '.SSSSSSGG.', '..SSSss...', '...sSS....'],
    back: ['..gggggg..', '.gHHHHHHg.', 'GGHHHhHHHH', 'GGGHHHHHHH', 'GGGHHHHHHH', '.GGHHHHHH.', '..SSSSSS..', '...SSSS...'] },
  reviewer: { pal: { S: '#f7dcc4', s: '#e0bea2', H: '#c9ccd4', h: '#e6e8ec', C: '#dd6a83', c: '#9c4257', P: '#5d6270' },
    front: ['.....HHH..', '..HHHhHHH.', '.HHHHHHHHH', '.HSSSSHHHH', '.kSkkSkSH.', '.SSSSSSsH.', '..SSSss...', '...sSS....'],
    back: ['....HHH...', '..HHhHHHH.', '.HHHHHHHHH', '.HHHHHHHHH', 'kSHHHHHHHH', '.sHHHHHHH.', '..SSSSSS..', '...SSSS...'] },
  tester: { pal: { S: '#e0ac7e', s: '#c48f62', H: '#15161a', h: '#2c2e35', C: '#8fc45a', c: '#5c8a35', P: '#4a5578' },
    front: ['..HHHHHH..', '.HHhHHHHH.', 'HbbHbbHHHH', 'HSSSSSHHHH', 'HSSSSSSHHH', 'HSSSSSSHH.', '.HSSSsHH..', '...sSS....'],
    back: ['..HHHHHH..', '.HHHhHHHH.', 'bbbbbbbbbH', 'HHHHHHHHHH', 'SHHHHHHHHH', 'HHHHHHHHH.', '.HSSSSSH..', '...SSSS...'] },
  reporter: { pal: { S: '#a9714b', s: '#8a5a3a', H: '#3a2a20', h: '#5a4232', C: '#b377d9', c: '#7d4a9e', P: '#6b5a4a' },
    front: ['..CCCCCC..', '.CCCCCCCC.', 'ccccCCCCCC', '.HSSSSHHH.', '.SSSSSSSH.', '.SSSSSSsH.', '..SSSss...', '...sSS....'],
    back: ['..CCCCCC..', '.CCCCCCCC.', 'CCCCCCCCCC', '.HHHHHHHH.', 'SHHHHHHHH.', '.sHHHHHH..', '..SSSSSS..', '...SSSS...'] },
  /* Code review: coque no alto, óculos escuros de leitura e camisa âmbar. */
  code_review: { pal: { S: '#d29a6e', s: '#b27d53', H: '#4a2c22', h: '#6e4434', C: '#d9a441', c: '#9c7226', P: '#3f4a63' },
    front: ['...HHHH...', '..HhHHHH..', '.HHHHHHHH.', 'HHSSSSSHHH', '.kkSkkSSH.', '.SSSSSSsH.', '..SSSss...', '...sSS....'],
    back: ['...HHHH...', '..HHHhHH..', '.HHHHHHHH.', 'HHHHHHHHHH', 'kHHHHHHHHH', '.sHHHHHHH.', '..SSSSSS..', '...SSSS...'] }
};
const EYES = ['e..e'];
const TORSO = ['..CCCCCC..', '.CCCCCCCC.', '.cCCCCCCc.', '.cCCCCCCc.', '..CCCCCC..'];
/* Sentada: o tronco desce atrás da mesa, que é pintada por cima (só aparece do tampo para cima). */
const SEATED = ['..CCCCCC..', '.CCCCCCCC.', '.cCCCCCCc.', '.cCCCCCCc.', '.cCCCCCCc.', '.cCCCCCCc.', '.cCCCCCCc.', '.cCCCCCCc.', '.cCCCCCCc.', '.cCCCCCCc.', '.cCCCCCCc.'];
/* Poses sentadas (dx, dy relativos ao quadril; mapas olhando para sudoeste). Antebraços sobre a mesa vão na camada da frente. */
const HAND_L = ['..cc', 'SS..'];
const HAND_R = ['...cc', '.cc..', 'SS...'];
const ARMS_CROSSED = ['.cccccccc.', '.SccccccS.'];
const MUG_SIP = ['.WW.......', '.WWSc.....', '.....c....', '......c...', '.......c..', '........c.'];
const WAVE = [['...S', '...c', '..c.', '..c.', '.c..', '.c..', 'c...', 'c...'], ['..S.', '..c.', '..c.', '..c.', '.c..', '.c..', 'c...', 'c...']];
const STRETCH = [['S........S', 'c........c', 'c........c', 'c........c', '.c......c.', '.c......c.', '.c......c.'], ['..........', 'S........S', 'c........c', 'c........c', 'c........c', '.c......c.', '.c......c.']];
const FACEPALM = ['...SS.....', '..SSSc....', '.....cc...', '.......c..', '.......cc.', '........c.'];
/* Caminhada (caixa 12x20, pés na última linha): quadris, ciclo de pernas e a pasta nas mãos. */
const HIPS = ['..PPPPPP..', '..PPPPPP..', '..PP..PP..'];
const STEP = ['..PP..PP..', '..PP..PP..', '..PP..PP..', '..WW..WW..'];
const LEG_FR = [STEP, ['..PP..PP..', '..PP..PP..', '..PP..WW..', '..WW......'], STEP, ['..PP..PP..', '..PP..PP..', '..WW..PP..', '......WW..']];
const ARMS_WALK = ['.c......c.', '.S......S.'];
const CARRY = ['..aa......', '.SaaaaaaS.', '.cAWWWWAc.', '..AAAAAA..'];

function walkerSprites(role, back, carry, frame) {
  const p = PEOPLE[role], still = frame != null, out = [], id = role + (back ? '-b' : '-f');
  const legs = i => SO('wk-legs-' + role + i, '', '', t => { t.map(1, 13, HIPS, p.pal); t.map(1, 16, LEG_FR[i], p.pal); });
  if (still) out.push(legs(frame)); else frames(4, 0.44, legs).forEach(s => out.push(s));
  const fold = carry && !back;
  out.push(SO('wk-up-' + id + (fold ? '-c' : ''), still ? '' : 'a-walkbob', '', t => {
    t.map(1, 8, TORSO, p.pal); if (fold) t.map(1, 9, CARRY, p.pal); else t.map(1, 10, ARMS_WALK, p.pal);
    t.map(1, 0, back ? p.back : p.front, p.pal); if (!back) t.map(3, 4, EYES, p.pal);
  }));
  return out;
}

/* Pessoa sentada atrás da mesa, de frente para a câmera. body vai antes da mesa (a mesa esconde o colo);
   hands vai depois (antebraços e mãos sobre o tampo). cls é a animação da camada do corpo. */
function seatedParts(role, v, i) {
  const p = PEOPLE[role], g = geo(role), id = '-' + role;
  const put = (dx, dy, lines) => t => kit(t, g).body(dx, dy, lines, p.pal);
  const body = [SO('torso' + id, '', '', t => { put(-5, -7, SEATED)(t); put(-5, -15, p.front)(t); })];
  const hands = [];
  const eyes = cls => body.push(S('eyes' + id, cls, '', put(-3, -11, EYES)));
  const sip = v === 'waiting' && i % 2 === 0, d = sip ? 'animation-delay:-' + ((i * 1.9) % 7).toFixed(1) + 's' : '';
  hands.push(mug(role, sip ? 'a-sipA' : '', d));
  let cls = 'a-bob';
  if (v === 'working') {
    cls = ''; eyes('a-wink');
    hands.push(SO('typeL' + id, 'a-tapL', '', put(-7, 0, HAND_L)));
    hands.push(SO('typeR' + id, 'a-tapR', '', put(-3, 1, HAND_R)));
  } else if (v === 'waiting') {
    eyes('a-wink');
    if (sip) {
      hands.push(SO('handL' + id, '', '', put(-7, 0, HAND_L)));
      hands.push(SO('handR' + id, 'a-sipA', d, put(-3, 1, HAND_R)));
      body.push(SO('sip' + id, 'a-sipB', d, put(-5, -11, MUG_SIP)));
    } else body.push(SO('crossed' + id, '', '', put(-5, -4, ARMS_CROSSED)));
  } else if (v === 'you') {
    eyes('a-wink');
    frames(2, 0.5, n => S('wave' + n + id, '', '', put(3, -15, WAVE[n]))).forEach(s => body.push(s));
    hands.push(SO('handL' + id, '', '', put(-7, 0, HAND_L)));
  } else if (v === 'done') {
    cls = ''; eyes('');
    frames(2, 1.8, n => SO('stretch' + n + id, '', '', put(-5, -19, STRETCH[n]))).forEach(s => body.push(s));
  } else if (v === 'failed') {
    cls = 'a-nod';
    body.push(SO('facepalm' + id, '', '', put(-5, -14, FACEPALM)));
    hands.push(SO('handL' + id, '', '', put(-7, 0, HAND_L)));
  } else {
    eyes('a-wink');
    hands.push(SO('handL' + id, '', '', put(-7, 0, HAND_L)));
    hands.push(SO('handR' + id, '', '', put(-3, 1, HAND_R)));
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
const drawScreen = (k, c) => k.side(0.4, 1.14, 1.86, 14, 22, c);
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
   estado novo a partir daí (portões g-until/g-from em --at). Quem saiu para entregar não aparece sentado. */
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
  const person = vis => (!you && vis !== 'skipped' && !c.away ? seatedParts(role, vis, c.index) : null);
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
    return b == null ? [] : [SO('badge-' + vis + id, b, '', t => t.map(g.seat[0] - 5, g.seat[1] - 27, BADGE[vis]))];
  };
  slot(badge, c.away ? c.td : null);
  if (v === 'failed' && !you) add([0, 0.6, 1.2].map((d, n) => S('smoke' + n + id, 'a-smoke', d ? 'animation-delay:' + d + 's' : '', t => {
    const p = g.pt(0.32, 1.5, 23); t.px(p[0] - 3 + n * 2, p[1] - 3 - (n % 2), 2 + (n % 2), 2 + (n % 2), n === 1 ? '#8b909a99' : '#8b909acc');
  })));
  return layers;
}

/* Cenário fixo, memoizado: piso, paredes, janelas e tapetes (viewBox 0 0 360 186) e os móveis soltos,
   cada um com o seu z-index pela área que ocupa no chão. */
const LEAVES = ['.....n.....', '....nN.....', '..N.nN..n..', '..NN.N.nN..', '...NNN.NN..', 'n...NNNN...', 'Nn..NNN..n.', '.NN.NNN.nN.', '..NNNNNNN..', '...NNNNN...', '....NNN....'];
const LEAF_PAL = { N: '#2f6b4f', n: '#4a9a6f' };
const SKY = [[17, 19.5, '#1f2547'], [15, 17, '#2c3468'], [13, 15, '#434c96'], [11.5, 13, '#6b63a8'], [10.5, 11.5, '#b0708a'], [9.5, 10.5, '#d9a441']];
function backdrop() {
  const G = GRID, FRAME = '#3a3e48', out = [];
  /* Janela numa parede: f = face da parede (faceY para a da direita, faceX para a da esquerda), entre p0 e p1. */
  const win = (f, p0, p1) => {
    f(p0, p1, 7.5, 21.5, FRAME);
    SKY.forEach(b => f(p0 + .12, p1 - .12, b[0], b[1], b[2]));
    [[.25, .5, 13], [.6, .8, 15.5], [.95, 1.3, 12], [1.4, 1.52, 14]].forEach(s => f(p0 + s[0], p0 + s[1], 9.5, s[2], '#14151a'));
    f((p0 + p1) / 2 - .04, (p0 + p1) / 2 + .04, 9.5, 19.5, FRAME); f(p0 + .12, p1 - .12, 14.4, 15, FRAME);
    f(p0 - .06, p1 + .06, 6.5, 7.5, '#4a4e57');
  };
  out.push(S('bg-room', '', '', t => {
    const wallR = (x0, x1, z0, z1, c) => t.faceY(0, x0, x1, z0, z1, c), wallL = (y0, y1, z0, z1, c) => t.faceX(0, y0, y1, z0, z1, c);
    t.faceY(G, 0, G, -4, 0, '#1b1d23'); t.faceX(G, 0, G, -4, 0, '#141519');
    t.top(0, 0, G, G, 0, '#24272f');
    for (let x = 0; x < G; x++) for (let y = 0; y < G; y++) t.top(x + .05, y + .05, x + .95, y + .95, 0, (x + y) % 2 ? '#2c2f39' : '#31343f');
    t.top(.6, 9.7, 3.6, 12.6, 0, '#3d447a'); t.top(.72, 9.82, 3.48, 12.48, 0, '#2d3254'); t.top(1, 10.1, 3.2, 12.2, 0, '#343a62'); t.top(1.12, 10.22, 3.08, 12.08, 0, '#2d3254');
    t.top(3.7, .1, 6.3, 2.9, 0, '#3a3f6e'); t.top(3.82, .22, 6.18, 2.78, 0, '#30355c');
    t.top(10.3, 5.2, 12.7, 8.8, 0, '#2f4a45'); t.top(10.42, 5.32, 12.58, 8.68, 0, '#27403b');
    wallR(0, G, 0, WALL, '#272a33'); wallL(0, G, 0, WALL, '#20232a');
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
  sprites.forEach(s => { const b = s.paths.box; x0 = Math.min(x0, b[0]); y0 = Math.min(y0, b[1]); x1 = Math.max(x1, b[0] + b[2]); y1 = Math.max(y1, b[1] + b[3]); });
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
  return memo('props', () => {
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
function scenery() { return memo('scenery', () => ({ backdrop: backdrop(), props: props() })); }

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
   corredores com a pasta, entrega na ponta da mesa seguinte, ao lado da cadeira, e volta. A mesa "você" não tem
   pessoa: de lá a pasta voa como aviãozinho de papel. Cada trecho reto é um elemento com posição e z-index animados
   em passos de 2x1 px; fora da sua janela de tempo ele fica com opacity 0. */
function legStyle(a, b, za, zb, t0, dur, steps) {
  return '--x0:' + a[0] + ';--y0:' + a[1] + ';--x1:' + b[0] + ';--y1:' + b[1] + ';--z0:' + za + ';--z1:' + zb +
    ';--t0:' + t0.toFixed(2) + 's;--dur:' + dur.toFixed(2) + 's;--ease:steps(' + steps + ',end)';
}
/* Canto superior esquerdo da caixa 12x20 de quem anda, com os pés no ponto do chão p. */
const feet = p => [sx(p[0], p[1]) - 6, sy(p[0], p[1]) - 20];
function tripOf(scene) {
  const ai = scene.active, pi = scene.previous;
  if (ai == null || pi == null || ai === pi || !scene.stations[ai] || !scene.stations[pi]) return null;
  const from = scene.stations[pi].role, go = geo(from), gd = geo(scene.stations[ai].role);
  const base = { id: scene.tripId || 0, fromIndex: pi, toIndex: ai };
  if (from === 'approval') {
    const p = go.w(1, 1.5), q = gd.w(1.1, 1.7), td = 1.8;
    const a = [sx(p[0], p[1]) - 6, sy(p[0], p[1], 16) - 6], b = [sx(q[0], q[1]) - 6, sy(q[0], q[1], 13) - 6];
    return Object.assign(base, { kind: 'plane', td: td, arrive: td, points: [p, q], legs: [{
      cls: 'leg plane f-' + (b[0] < a[0] ? 'sw' : 'se'), vb: '0 0 12 6',
      style: legStyle(a, b, 2900, 2900, 0, td, 36), sprites: [SO('plane', '', '', t => t.map(0, 0, PLANE))]
    }] });
  }
  const B = blocked(), goal = [Math.floor(gd.front[0]), Math.floor(gd.front[1])];
  let best = null;
  go.exits.forEach(e => {
    const out = [Math.floor(e.out[0]), Math.floor(e.out[1])];
    if (out[0] < 0 || out[1] < 0 || B[out[0] + ',' + out[1]]) return;
    const r = route(out, goal); if (!r) return;
    const pts = [e.stand].concat(r.map(q => [q[0] + .5, q[1] + .5]));
    if (!best || pts.length < best.length) best = pts;
  });
  if (!best) return null;
  const corners = best.filter((q, i) => {
    if (i === 0 || i === best.length - 1) return true;
    const a = best[i - 1], b = best[i + 1];
    return (q[0] - a[0]) * (b[1] - q[1]) !== (q[1] - a[1]) * (b[0] - q[0]);
  });
  const legs = [], face = (p, q) => (q[0] > p[0] ? 'se' : q[0] < p[0] ? 'nw' : q[1] > p[1] ? 'sw' : 'ne');
  let t = 0;
  /* Um trecho: andando (ciclo de 4 quadros, um passo de 2x1 px a cada 1/6 de tile) ou parado entregando a pasta. */
  const leg = (p, q, dir, carry) => {
    const len = Math.abs(q[0] - p[0]) + Math.abs(q[1] - p[1]), dur = len ? len / SPEED : HAND;
    legs.push({ cls: 'leg f-' + dir, vb: '0 0 12 20', style: legStyle(feet(p), feet(q), depth(p), depth(q), t, dur, Math.max(1, Math.round(len * 6))), sprites: walkerSprites(from, dir === 'ne' || dir === 'nw', carry, len ? null : 0) });
    t += dur;
  };
  for (let i = 0; i + 1 < corners.length; i++) leg(corners[i], corners[i + 1], face(corners[i], corners[i + 1]), true);
  const arrive = t, end = corners[corners.length - 1];
  leg(end, end, gd.facing, false);
  for (let i = corners.length - 1; i > 0; i--) leg(corners[i], corners[i - 1], face(corners[i], corners[i - 1]), false);
  return Object.assign(base, { kind: 'walk', td: +t.toFixed(2), arrive: +arrive.toFixed(2), points: corners, legs: legs });
}

/* Estado da cena -> tudo o que a marcação precisa. opt: { scale, traveling }.
   traveling = true enquanto a viagem atual anda; quem monta o componente desliga depois de trip.td segundos. */
function buildView(scene, opt) {
  opt = opt || {};
  const trip = opt.traveling ? tripOf(scene) : null;
  const stations = scene.stations.map((st, i) => {
    const g = geo(st.role), selected = scene.selected === i, active = scene.active === i;
    const from = !!trip && trip.fromIndex === i, to = !!trip && trip.toIndex === i;
    const layers = stationLayers(st, {
      index: i, selected: selected, folder: active && st.visual !== 'skipped',
      to: to, arrive: trip ? trip.arrive : 0, away: from && trip.kind === 'walk', td: trip ? trip.td : 0
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
  return { roomStyle: '--s:' + (opt.scale || 3), scenery: scenery(), stations: stations, trip: trip };
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
}

interface Sprite { k: string; cls: string; style: string; paths: { f: string; d: string }[] }
interface Layer { cls: string; style: string; sprites: Sprite[] }

export interface OfficeTrip { id: number; kind: "walk" | "plane"; td: number; legs: { cls: string; style: string; vb: string; sprites: Sprite[] }[] }

export interface OfficeView {
  roomStyle: string;
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
}

/** Largura e altura lógicas da sala. */
export const OFFICE_W: number = LW;
export const OFFICE_H: number = LH;
/** `traveling`: verdadeiro enquanto a viagem atual anda; quem monta desliga depois de `trip.td` segundos. */
export const officeView: (scene: OfficeScene, opt: { scale: number; traveling: boolean }) => OfficeView = buildView;
export const officeTrip: (scene: OfficeScene) => OfficeTrip | null = tripOf;

const spritesSvg = (list: Sprite[]): string =>
  list.map((s) => `<g class="${s.cls}" style="${s.style}">${s.paths.map((p) => `<path d="${p.d}" fill="${p.f}"/>`).join("")}</g>`).join("");

/** Camadas de uma estação como SVG, para {@html} dentro de um <svg>. */
export function layersSvg(layers: Layer[]): string {
  return layers.map((l) => `<g class="${l.cls}" style="${l.style}">${spritesSvg(l.sprites)}</g>`).join("");
}
/** Sprites soltos (trechos da viagem) como SVG. */
export { spritesSvg };

/** Cenário fixo da sala: o fundo como SVG e os móveis (cada um com a sua profundidade). Gerado uma vez só. */
let sceneryCache: { backdrop: string; props: { style: string; vb: string; svg: string }[] } | null = null;
export function officeScenery() {
  if (!sceneryCache) {
    const s = scenery();
    sceneryCache = {
      backdrop: spritesSvg(s.backdrop),
      props: s.props.map((p) => ({ style: p.style, vb: p.vb, svg: spritesSvg(p.sprites) })),
    };
  }
  return sceneryCache;
}
