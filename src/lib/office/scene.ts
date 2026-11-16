// @ts-nocheck
/* Cena do escritório em pixel art da esteira de bots, vinda do protótipo no Claude Design.
   Grade lógica 360x136 px escalada por inteiro (--s). Sprites são mapas de caracteres convertidos
   em um <path> de retângulos por cor; a animação fica toda no CSS (steps()) do PipelineOffice.
   Funções puras: estado da cena -> modelo de desenho. Os tipos públicos ficam no fim do arquivo. */
const LW = 360, LH = 136, SLOT = 48, X0 = 12;

/* Paleta: uma letra por cor nos mapas de pixels. M, L e D vêm do bot (BOT). */
const PAL = { K: '#101114', G: '#8b909a', g: '#5a5f69', W: '#ecedef', w: '#b4b8c0', F: '#161a24', E: '#eaf6ff', A: '#d9a441', a: '#e3ba6a', q: '#9c7226', N: '#58b88a', n: '#86d3a6', R: '#d9645a', r: '#efa49b', B: '#5b6ad9', b: '#a6b0f7', P: '#d8dbe2', S: '#12151d', T: '#7d6247', t: '#94765a', H: '#5e4735', h: '#473629' };
const BOT = {
  triage: { M: '#49b5b0', L: '#86d8d2', D: '#2c7f7b' },
  planner: { M: '#6f7de0', L: '#a6b0f7', D: '#4450a8' },
  implementer: { M: '#e0894e', L: '#f2b584', D: '#a85f30' },
  reviewer: { M: '#dd6a83', L: '#f0a3b4', D: '#9c4257' },
  tester: { M: '#8fc45a', L: '#b9e08c', D: '#5c8a35' },
  reporter: { M: '#b377d9', L: '#d6a8f0', D: '#7d4a9e' }
};

/* Cabeças 12x10: cada papel tem a sua silhueta. */
const HEAD = {
  triage: ['......a.....', '......g.....', '...LLLLLL...', '..LMMMMMMD..', '.LMMFFFFMMD.', '.LMFFEEFFMD.', '.LMFFEEFFMD.', '.LMMFFFFMMD.', '..MMMMMMMD..', '...DDDDDD...'],
  planner: ['..g......g..', '..g......g..', '.LLLLLLLLLL.', '.LMMMMMMMMD.', '.LFFFFFFFFD.', 'gLFEEFFEEFDg', 'gLFEEFFEEFDg', '.LFFFFFFFFD.', '.LMMMMMMMMD.', '.MDDDDDDDDD.'],
  implementer: ['............', '...gggggg...', '..gLLLLLLg..', '.gLMMMMMMDg.', 'KKMMMMMMMMKK', 'KKFFFFFFFFKK', 'KKEEEEEEEEKK', 'KKFFFFFFFFKK', '..MMMMMMMD..', '..DDDDDDDD..'],
  reviewer: ['............', '....LLLL....', '...LMMMMD...', '..LMMMMMMD..', '..LMMMMMMD..', '.WWWWMMWWWW.', '.WFEWWWWFEW.', '.WWWWMMWWWW.', '..MMMMMMMD..', '...DDDDDD...'],
  tester: ['.....nn.....', '.....NN.....', '....LLLL....', '...LMMMMD...', '..LMFFFFMD..', '.LMFEFFEFMD.', '.LMFFFFFFMD.', 'LMMFFNNFFMMD', 'LMMMMMMMMMMD', '.DDDDDDDDDD.'],
  reporter: ['............', '...LLLLLL...', '..LMMMMMMD..', 'g.LMMMMMMD.g', 'GgLFEFFEFDgG', 'GgLFFFFFFDgG', 'GgLMwwwwMDgG', 'g.LMMMMMMD.g', '..MMwwwwMD..', '...DDDDDD...']
};
/* Pálpebras: retângulos [x, y, w, h] na grade da cabeça (piscar, relaxar, desligar). */
const LIDS = { triage: [[5, 5, 2, 1]], planner: [[3, 5, 2, 1], [7, 5, 2, 1]], implementer: [[2, 6, 3, 1], [7, 6, 3, 1]], reviewer: [[3, 6, 1, 1], [9, 6, 1, 1]], tester: [[4, 5, 1, 1], [7, 5, 1, 1]], reporter: [[4, 4, 1, 1], [7, 4, 1, 1]] };
const TORSO = ['....gggg....', '..LMMMMMMD..', '.LMMMMMMMMD.', '.LMMFEEFMMD.', '.LMMFFFFMMD.', '.LMMMMMMMMD.', '.MMMMMMMMMD.', '.DDDDDDDDDD.'];
const ARM_L = ['MM..', 'MM..', 'MD..', 'MD..', 'MD..', 'MD..', 'MD..', '.DD.', '..GG', '..GG'];
const ARM_R = ['..MM', '..MM', '..MD', '..MD', '..MD', '..MD', '..MD', '.DD.', 'GG..', 'GG..'];
const RELAX_L = ['...GG', '..MM.', '.MM..', 'MM...', 'MM...', '.MM..', '..MM.', '...MM'];
const RELAX_R = ['GG...', '.MM..', '..MM.', '...MM', '...MM', '..MM.', '.MM..', 'MM...'];
const ARM_SIP = ['WWW...', 'WaW...', 'WWWGG.', '...MM.', '....MM', '....MM', '....MM'];
const ARM_CARRY = ['MD', 'MD', 'MD', 'GG'];
const MUG = ['WWW.', 'WaWW', 'WWW.'];
const STEAM = [['.w..', 'w...', '.w..'], ['w...', '.w..', 'w...']];
const KEYB = ['GGGGGGGGGG', 'GwGwGwGwGG', 'gggggggggg'];
const KEYB_RGB = ['GGGGGGGGGG', 'GbGnGaGrGG', 'gggggggggg'];
const LEG_STAND = ['...DD..DD...', '...DD..DD...', '...DD..DD...', '...DD..DD...', '..ggg..ggg..'];
const LEGS = [LEG_STAND, ['...DD..DD...', '...DD..DD...', '...DD..DD...', '..ggg..DD...', '.......ggg..'], LEG_STAND, ['...DD..DD...', '...DD..DD...', '...DD..DD...', '...DD..ggg..', '..ggg.......']];
const FOLDER = ['.aaaa.....', 'aaaaaaaaaa', 'aPPPPPPPPa', 'AAAAAAAAAA', 'AAAAAAAAAA', 'AAAAAAAAAA', 'AAAAAAAAAA', 'qqqqqqqqqq'];
const FOLDER_S = ['.aaa....', 'aaaaaaaa', 'APPPPPPA', 'AAAAAAAA', 'AAAAAAAA', 'qqqqqqqq'];
const PLANE = ['...WW.......', '...wWWW.....', '....wWWWW...', '.....wWWWWW.', '.....wwWW...', '......w.....'];
const BADGE = {
  you: ['.AAAAAAA.', 'AAAAKAAAA', 'AAAAKAAAA', 'AAAAKAAAA', 'AAAAKAAAA', 'AAAAAAAAA', 'AAAAKAAAA', '.AAAAAAA.', '....A....'],
  done: ['.NNNNNNN.', 'NNNNNNNNN', 'NNNNNNKNN', 'NNNNNKNNN', 'NKNNKNNNN', 'NNKKNNNNN', 'NNNNNNNNN', '.NNNNNNN.'],
  returned: ['.rrrrrrr.', 'rrrrrrrrr', 'rrKrrrrrr', 'rKKKKKrrr', 'rrKrrrKrr', 'rrrrrrKrr', 'rrrKKKrrr', '.rrrrrrr.'],
  failed: ['.RRRRRRR.', 'RRRRRRRRR', 'RRWRRRWRR', 'RRRWRWRRR', 'RRRRWRRRR', 'RRRWRWRRR', 'RRWRRRWRR', '.RRRRRRR.']
};
/* Acessórios de mesa de cada papel. */
const LUPA = ['.GGG...', 'G.b.G..', 'Gb.bG..', 'G.b.G..', '.GGGg..', '....gg.', '.....g.'];
const TRI_PAPER = ['PPPPPP', 'PwwwPP', 'PPPPPP', 'PwwPPP'];
const CLIP = ['..ggg..', '.qqqqq.', '.qPPPq.', '.qbbPq.', '.qPPPq.', '.qbbPq.', '.qPPPq.', '.qbPPq.', '.qPPPq.', '.qqqqq.'];
const PENCIL = ['a', 'a', 'a', 'K'];
const WRENCH = ['G.G', 'G.G', 'GGG', '.w.', '.G.', '.G.', '.G.'];
const REV_PAPER = ['PPPPPPP', 'PnnnPPP', 'PrrrrPP', 'PnnPPPP', 'PPPPPPP'];
const PEN = ['RR', 'RR', 'RR', 'RR', '.K'];
const RACK = ['.W.W.W.', '.W.W.W.', '.n.W.n.', '.N.n.N.', '.N.N.N.', '.N.N.N.', '.w.w.w.', 'ggggggg', 'g.....g'];
const MEGA = ['.RR.....', 'WRRRRR..', 'WRRRRRRg', 'WRRRRRRg', 'WRRRRR..', '.RR.....'];
const REP_PAPER = ['PPPPP', 'PbbbP'];
/* Mesa "você": luminária, campainha e plaquinha. */
const LAMP = ['..XXXX..', '.XXXXXX.', 'xxxxxxxx'];
const BELL = ['...K...', '..AAA..', '.AaAAA.', 'AAAAAAA', 'ggggggg'];
const GLYPH = { V: ['X.X', 'X.X', 'X.X', 'X.X', '.X.'], O: ['XXX', 'X.X', 'X.X', 'X.X', 'XXX'], C: ['XXX', 'X..', 'X..', 'X..', 'XXX'], E: ['.X.', 'X.X', 'XXX', 'X..', 'XX.', 'X..', 'XXX'] };
/* Cenário. */
const CLOCK = ['...ggggg...', '.ggPPPPPgg.', '.gPPPPPPPg.', 'gPPPPPPPPPg', 'gPPPPPPPPPg', 'gPPPPKPPPPg', 'gPPPPPPPPPg', 'gPPPPPPPPPg', '.gPPPPPPPg.', '.ggPPPPPgg.', '...ggggg...'];
const LEAF_TOP = ['.....n.....', '....nN.....', '..N.nN..n..', '..NN.N.nN..', '...NNN.NN..'];
const LEAF_LOW = ['n...NNNN...', 'Nn..NNN..n.', '.NN.NNN.nN.', '..NNNNNNN..', '...NNNNN...', '....NNN....', '....NNN....'];
const LEAF_PAL = { N: '#2f6b4f', n: '#4a9a6f' };
const CODE = [[0, 5, 'b'], [2, 6, 'n'], [2, 4, 'w'], [4, 3, 'a'], [2, 5, 'b'], [0, 3, 'r'], [0, 6, 'n'], [2, 4, 'w']];

/* Pintura: grade esparsa de pixels -> um <path> de retângulos por cor (ordem de pintura resolvida na grade). */
function paint(g, x, y, w, h, c) {
  for (let j = y; j < y + h; j++) { const row = g[j] || (g[j] = {}); for (let i = x; i < x + w; i++) row[i] = c; }
}
function stamp(g, x, y, lines, pal) {
  for (let j = 0; j < lines.length; j++) {
    const ln = lines[j];
    for (let i = 0; i < ln.length; i++) {
      const ch = ln[i]; if (ch === '.') continue;
      const c = (pal && pal[ch]) || PAL[ch]; if (c) paint(g, x + i, y + j, 1, 1, c);
    }
  }
}
function toPaths(g) {
  const rects = {}, order = [], open = {};
  Object.keys(g).map(Number).sort((a, b) => a - b).forEach(y => {
    const row = g[y], xs = Object.keys(row).map(Number).sort((a, b) => a - b);
    for (let i = 0; i < xs.length;) {
      const x = xs[i], c = row[x]; let w = 1;
      while (i + w < xs.length && xs[i + w] === x + w && row[x + w] === c) w++;
      const k = c + '|' + x + '|' + w, o = open[k];
      if (o && o.y + o.h === y) o.h++;
      else { const r = { x: x, y: y, w: w, h: 1 }; open[k] = r; if (!rects[c]) { rects[c] = []; order.push(c); } rects[c].push(r); }
      i += w;
    }
  });
  return order.map(c => ({ f: c, d: rects[c].map(r => 'M' + r.x + ' ' + r.y + 'h' + r.w + 'v' + r.h + 'h-' + r.w + 'z').join('') }));
}
const MEMO = {};
function art(key, draw) {
  if (!MEMO[key]) {
    const g = {};
    draw((x, y, w, h, c) => paint(g, x, y, w, h, c), (x, y, lines, pal) => stamp(g, x, y, lines, pal));
    MEMO[key] = toPaths(g);
  }
  return MEMO[key];
}
/* Camada = um <g>: chave do desenho, classes de animação, estilo inline e os paths. */
function L(key, cls, style, draw) { return { k: key, cls: cls || '', style: style || '', paths: art(key, draw) }; }
/* Quadros alternados por visibilidade: n camadas, cada uma visível em 1/n do ciclo. */
function frames(n, dur, make) {
  const out = [];
  for (let k = 0; k < n; k++) {
    const l = make(k);
    l.cls = 'fr fr' + n + (k === 0 ? ' fr0' : '') + (l.cls ? ' ' + l.cls : '');
    l.style = 'animation-duration:' + dur + 's;animation-delay:' + (dur * k / n).toFixed(3) + 's';
    out.push(l);
  }
  return out;
}
function botPal(role) { const b = BOT[role]; return { M: b.M, L: b.L, D: b.D }; }

function drawDesk(box, map, you, knob) {
  box(4, 32, 40, 8, PAL.T); box(4, 32, 40, 1, PAL.t);
  box(4, 40, 40, 14, PAL.H); box(4, 40, 40, 1, PAL.h);
  box(5, 54, 3, 4, PAL.h); box(40, 54, 3, 4, PAL.h); box(4, 58, 40, 1, '#0000004d');
  if (you) {
    box(17, 43, 19, 10, '#1d1f24'); box(17, 43, 19, 1, '#33363e');
    const ink = { X: '#b4b8c0' };
    map(19, 46, GLYPH.V, ink); map(23, 46, GLYPH.O, ink); map(27, 46, GLYPH.C, ink); map(31, 44, GLYPH.E, ink);
  } else {
    box(31, 43, 10, 8, '#553f2f'); box(31, 43, 10, 1, PAL.h); box(35, 46, 2, 1, knob);
  }
}
function drawCode(box, k, term) {
  box(29, 23, 11, 8, PAL.S);
  for (let j = 0; j < 3; j++) {
    const ln = CODE[(k + j) % CODE.length];
    box(30 + ln[0], 24 + j * 2, ln[1], 1, term ? PAL.N : PAL[ln[2]]);
    if (term) box(30, 24 + j * 2, 1, 1, PAL.n);
  }
  if (k % 2 === 0) { const last = CODE[(k + 2) % CODE.length]; box(31 + last[0] + last[1], 28, 1, 1, PAL.W); }
}
function screenLayers(role, v) {
  const term = role === 'tester';
  if (v === 'working') return frames(3, term ? 0.9 : 0.72, k => L('scr-code-' + (term ? 't' : 'c') + k, '', '', box => drawCode(box, k, term)));
  if (v === 'failed') return [
    L('scr-fail', '', '', box => box(29, 23, 11, 8, '#4a1717')),
    L('scr-fail-hot', 'a-flick', '', box => box(29, 23, 11, 8, '#7a2222')),
    L('scr-fail-x', '', '', (box, map) => map(32, 24, ['r...r', '.r.r.', '..r..', '.r.r.', 'r...r']))
  ];
  if (v === 'done') return [L('scr-done', '', '', (box, map) => { box(29, 23, 11, 8, PAL.S); map(32, 25, ['....N', '...N.', 'N.N..', '.N...']); })];
  if (v === 'skipped') return [L('scr-off', '', '', box => box(29, 23, 11, 8, '#0b0c0f'))];
  if (v === 'you') return [
    L('scr-ask', '', '', box => { box(29, 23, 11, 8, PAL.S); box(30, 24, 5, 1, PAL.A); box(30, 26, 3, 1, PAL.A); }),
    L('scr-ask-cur', 'a-flick', '', box => box(34, 26, 1, 1, PAL.a))
  ];
  return [L('scr-idle', '', '', box => box(29, 23, 11, 8, PAL.S)), L('scr-led', 'a-led', '', box => box(38, 29, 1, 1, PAL.g))];
}
function accessoryLayers(role, working) {
  const o = [];
  if (role === 'triage') {
    o.push(L('acc-tri-paper', '', '', (box, map) => map(5, 34, TRI_PAPER)));
    o.push(L('acc-tri-lupa', working ? 'a-scan' : '', '', (box, map) => map(4, 28, LUPA)));
  } else if (role === 'planner') {
    o.push(L('acc-plan-clip', '', '', (box, map) => map(5, 26, CLIP)));
    o.push(L('acc-plan-pencil', working ? 'a-write' : '', '', (box, map) => map(11, 31, PENCIL)));
  } else if (role === 'implementer') {
    o.push(L('acc-impl-wrench', '', '', (box, map) => map(6, 30, WRENCH)));
  } else if (role === 'reviewer') {
    o.push(L('acc-rev-paper', '', '', (box, map) => map(4, 33, REV_PAPER)));
    o.push(L('acc-rev-pen', working ? 'a-write' : '', '', (box, map) => map(10, 29, PEN)));
  } else if (role === 'tester') {
    o.push(L('acc-test-rack', '', '', (box, map) => map(4, 27, RACK)));
    if (working) frames(2, 0.7, k => L('acc-test-bub' + k, '', '', box => {
      if (k === 0) { box(5, 25, 1, 1, PAL.n); box(9, 26, 1, 1, PAL.n); } else { box(5, 24, 1, 1, PAL.n); box(7, 25, 1, 1, PAL.n); box(9, 24, 1, 1, PAL.n); }
    })).forEach(l => o.push(l));
  } else if (role === 'reporter') {
    o.push(L('acc-rep-mega', '', '', (box, map) => map(4, 28, MEGA)));
    o.push(L('acc-rep-paper', '', '', (box, map) => map(5, 36, REP_PAPER)));
    if (working) frames(3, 0.9, k => L('acc-rep-wave' + k, '', '', box => box(2 - k, 30 - k, 1, 2 + k * 2, PAL.a))).forEach(l => o.push(l));
  }
  return o;
}

/* Camadas de uma estação (viewBox 0 0 48 60), na ordem de pintura. c = contexto calculado em buildView. */
function stationLayers(st, c) {
  const role = st.role, you = role === 'approval';
  const v = you && st.visual === 'working' ? 'you' : st.visual;
  const out = [], push = l => { out.push(l); };
  const hasBot = !you && v !== 'skipped';
  const aw = c.away ? 'a-away' : '', awS = c.away ? 'animation-duration:' + c.td + 's' : '';
  const join = (a, b) => (a && b ? a + ' ' + b : a || b);
  const late = d => (d ? 'animation-delay:' + d + 's' : '');
  const P = you ? null : botPal(role);

  if (you) push(L('chair-you', '', '', box => { box(12, 19, 14, 15, '#4450a8'); box(12, 19, 14, 1, '#7b88e6'); box(14, 21, 10, 9, '#5b6ad9'); box(10, 28, 2, 6, '#33363e'); box(26, 28, 2, 6, '#33363e'); }));
  else push(L('chair', '', '', box => { box(11, 22, 16, 13, '#2c3038'); box(11, 22, 16, 1, '#3d424d'); }));

  if (hasBot) {
    if (v === 'done') push(L('relax-' + role, aw, awS, (box, map) => { map(9, 19, RELAX_L, P); map(24, 19, RELAX_R, P); }));
    push(L('torso-' + role, aw, awS, (box, map) => map(13, 24, TORSO, P)));
    const headCls = v === 'failed' ? 'a-droop' : (v === 'working' || v === 'done' || c.away) ? '' : 'a-bob';
    push(L('head-' + role, join(headCls, aw), awS, (box, map) => map(13, 14, HEAD[role], P)));
    const closed = v === 'done' || v === 'failed';
    if (closed || !c.away) push(L('lids-' + role, closed ? join(v === 'failed' ? 'a-droop' : '', aw) : 'a-blink', closed ? awS : '', box => LIDS[role].forEach(r => box(13 + r[0], 14 + r[1], r[2], r[3], PAL.F))));
  }

  push(L(you ? 'desk-you' : 'desk-' + role, '', '', (box, map) => drawDesk(box, map, you, you ? null : BOT[role].L)));

  if (you) {
    const on = v === 'you' || v === 'done';
    push(L('lamp-base', '', '', box => { box(8, 24, 1, 8, PAL.g); box(7, 32, 3, 1, PAL.g); box(6, 33, 5, 1, PAL.G); }));
    push(L(on ? 'lamp-on' : 'lamp-off', '', '', (box, map) => map(5, 21, LAMP, on ? { X: PAL.A, x: PAL.a } : { X: '#4a4e57', x: '#3d424d' })));
    if (on) push(L('lamp-cone', v === 'you' ? 'a-flick' : '', '', box => { box(5, 24, 8, 2, '#d9a44140'); box(4, 26, 10, 3, '#d9a44130'); box(3, 29, 12, 3, '#d9a44122'); box(2, 32, 14, 3, '#d9a44116'); }));
    push(L('bell', v === 'you' ? 'a-shake' : '', '', (box, map) => map(31, 31, BELL)));
    if (v === 'you') frames(2, 0.36, k => L('bell-ring' + k, '', '', box => {
      if (k === 0) { box(29, 32, 1, 1, PAL.a); box(39, 32, 1, 1, PAL.a); } else { box(28, 31, 1, 1, PAL.a); box(29, 34, 1, 1, PAL.a); box(40, 31, 1, 1, PAL.a); box(39, 34, 1, 1, PAL.a); }
    })).forEach(push);
  } else {
    push(L('monitor', '', '', box => { box(28, 22, 13, 10, '#2b2e36'); box(28, 22, 13, 1, '#3d424d'); box(41, 23, 1, 9, '#1b1d22'); box(33, 32, 3, 2, '#23252b'); box(31, 34, 7, 1, '#33363e'); }));
    screenLayers(role, v).forEach(push);
    push(L(role === 'implementer' ? 'keyb-rgb' : 'keyb', '', '', (box, map) => map(14, 36, role === 'implementer' ? KEYB_RGB : KEYB)));
    accessoryLayers(role, v === 'working').forEach(push);
    if (hasBot) {
      const armL = cls => L('armL-' + role, join(cls, aw), awS, (box, map) => map(12, 26, ARM_L, P));
      const armR = cls => L('armR-' + role, join(cls, aw), awS, (box, map) => map(22, 26, ARM_R, P));
      const mug = () => L('mug', '', '', (box, map) => map(25, 33, MUG));
      const steam = () => frames(2, 1.1, k => L('steam' + k, '', '', (box, map) => map(25, 29, STEAM[k])));
      if (v === 'working') { push(armL('a-tapL')); push(armR('a-tapR')); push(mug()); }
      else if (v === 'waiting' && !c.away) {
        const d = 'animation-delay:-' + ((c.i * 1.9) % 7).toFixed(1) + 's';
        push(armL(''));
        push(L('sipA-' + role, 'a-sipA', d, (box, map) => { map(22, 26, ARM_R, P); map(25, 33, MUG); }));
        push(L('sipB-' + role, 'a-sipB', d, (box, map) => map(20, 20, ARM_SIP, P)));
      } else if (v === 'done') { push(mug()); steam().forEach(push); }
      else { push(armL('')); push(armR('')); push(mug()); if (v === 'you' || v === 'returned') steam().forEach(push); }
    }
    if (v === 'failed') [0, 0.6, 1.2].forEach((d, k) => push(L('smoke' + k, 'a-smoke', late(d), box => box(31 + k * 3, 19 - (k % 2), 2 + (k % 2), 2 + (k % 2), k === 1 ? '#8b909a99' : '#8b909acc'))));
  }

  if (c.folder) push(L('folder', c.folderDelay ? 'a-arrive' : '', late(c.folderDelay), (box, map) => map(6, 43, FOLDER)));
  const badgeCls = { you: 'a-bounce', done: 'a-pop', returned: '', failed: '' }[v];
  if (badgeCls != null) push(L('badge-' + v, c.badgeDelay ? 'a-arrive' : badgeCls, late(c.badgeDelay), (box, map) => map(15, 2, BADGE[v])));
  if (c.selected) push(L('sel', 'a-sel', '', box => {
    const b = PAL.b;
    box(1, 12, 5, 1, b); box(1, 12, 1, 5, b); box(42, 12, 5, 1, b); box(46, 12, 1, 5, b);
    box(1, 58, 5, 1, b); box(1, 54, 1, 5, b); box(42, 58, 5, 1, b); box(46, 54, 1, 5, b);
  }));
  return out;
}

/* Cenário fixo da sala (viewBox 0 0 360 136): parede, piso, tapete, janelas e detalhes vivos. */
function drawWindow(box, x) {
  box(x, 6, 30, 28, '#3a3e48');
  [[8, 6, '#1f2547'], [14, 5, '#2c3468'], [19, 4, '#434c96'], [23, 3, '#6b63a8'], [26, 2, '#b0708a'], [28, 2, '#d9a441']].forEach(b => box(x + 2, b[0], 26, b[1], b[2]));
  [[3, 25, 4, 5], [8, 22, 3, 8], [12, 26, 5, 4], [18, 21, 4, 9], [23, 24, 4, 6]].forEach(s => box(x + s[0], s[1], s[2], s[3], '#14151a'));
  box(x + 9, 24, 1, 1, PAL.a); box(x + 19, 26, 1, 1, PAL.a);
  box(x + 4, 10, 1, 1, PAL.b); box(x + 25, 9, 1, 1, PAL.b);
  box(x + 14, 8, 1, 22, '#3a3e48'); box(x + 2, 18, 26, 1, '#3a3e48');
  box(x - 2, 34, 34, 2, '#4a4e57');
}
function backdrop() {
  if (MEMO._backdrop) return MEMO._backdrop;
  const out = [], WINS = [141, 237];
  out.push(L('bg-room', '', '', (box, map) => {
    box(0, 0, 360, 58, '#191b21'); box(0, 0, 360, 3, '#121317');
    for (let x = 30; x < 360; x += 30) box(x, 3, 1, 37, '#1d1f26');
    box(0, 40, 360, 14, '#1c1e25'); box(0, 40, 360, 1, '#272a33'); box(0, 54, 360, 4, '#272a33'); box(0, 57, 360, 1, '#101114');
    box(0, 58, 360, 60, '#26221f');
    for (let r = 0; r < 6; r++) {
      const y = 58 + r * 10;
      box(0, y + 9, 360, 1, '#1e1b18');
      for (let x = (r % 2 ? 14 : 44); x < 360; x += 60) box(x, y, 1, 9, '#1e1b18');
      if (r % 2 === 0) box((r * 47) % 300 + 20, y + 4, 22, 1, '#2d2925');
    }
    box(8, 99, 344, 17, '#1f2233'); box(8, 99, 344, 1, '#2d3254'); box(8, 115, 344, 1, '#2d3254'); box(8, 99, 1, 17, '#2d3254'); box(351, 99, 1, 17, '#2d3254');
    for (let x = 20; x < 344; x += 16) { box(x, 107, 2, 1, '#2d3254'); box(x + 8, 103, 1, 1, '#262a45'); box(x + 8, 111, 1, 1, '#262a45'); }
    box(0, 118, 360, 18, '#0e0f11'); box(0, 118, 360, 1, '#25272d');
    WINS.forEach(x => drawWindow(box, x));
    map(55, 9, CLOCK); box(60, 11, 1, 3, PAL.K); box(61, 14, 2, 1, PAL.K);
    box(95, 8, 26, 20, '#2b2e36'); box(96, 9, 24, 18, '#17181c'); box(96, 9, 24, 2, '#23252b'); box(104, 9, 1, 18, '#2b2e36'); box(112, 9, 1, 18, '#2b2e36');
    box(98, 12, 4, 3, PAL.a); box(98, 16, 4, 3, PAL.b); box(98, 20, 4, 3, PAL.a); box(98, 24, 4, 2, PAL.r);
    box(106, 12, 4, 3, PAL.b); box(114, 12, 4, 3, PAL.n); box(114, 16, 4, 3, PAL.n); box(114, 20, 4, 3, PAL.n);
    box(198, 30, 13, 28, '#23252b'); box(198, 30, 13, 1, '#3d424d'); box(210, 31, 1, 27, '#17181c');
    for (let k = 0; k < 5; k++) box(199, 35 + 5 * k, 11, 1, '#17181c');
    box(200, 32, 1, 1, PAL.n); box(202, 37, 1, 1, PAL.b); box(200, 42, 1, 1, PAL.n); box(204, 47, 1, 1, PAL.a); box(200, 52, 1, 1, PAL.n);
    box(288, 22, 24, 2, PAL.H); box(288, 22, 24, 1, PAL.T); box(290, 24, 1, 3, PAL.h); box(309, 24, 1, 3, PAL.h);
    box(289, 14, 2, 8, '#6f7de0'); box(291, 15, 2, 7, '#dd6a83'); box(293, 13, 3, 9, '#49b5b0'); box(296, 16, 2, 6, '#e3ba6a'); box(298, 14, 2, 8, '#8fc45a');
    box(304, 19, 4, 3, '#7a5240'); box(305, 15, 2, 4, '#3f8a63'); box(304, 16, 1, 2, '#3f8a63'); box(307, 17, 1, 1, '#3f8a63');
    box(2, 50, 8, 8, '#7a5240'); box(1, 49, 10, 2, '#8f6450'); box(2, 57, 8, 1, '#5a3c2e'); map(0, 42, LEAF_LOW, LEAF_PAL);
    box(350, 30, 7, 10, '#3f6f8f'); box(351, 31, 2, 8, '#6fa6c8'); box(352, 40, 3, 2, '#6fa6c8');
    box(349, 42, 9, 16, '#c9ced8'); box(356, 42, 2, 16, '#9aa0ab'); box(351, 45, 5, 4, '#2b2e36'); box(352, 47, 1, 1, PAL.r); box(354, 47, 1, 1, PAL.b); box(350, 50, 7, 1, '#8b909a'); box(349, 57, 9, 1, '#6c717c');
  }));
  out.push(L('bg-light', '', '', box => WINS.forEach(x => { for (let i = 0; i < 6; i++) box(x - 4 - i * 3, 58 + i * 7, 34, 7, '#a6b0f70b'); })));
  out.push(L('bg-leaf', 'a-sway', '', (box, map) => map(0, 37, LEAF_TOP, LEAF_PAL)));
  WINS.forEach((x, w) => {
    out.push(L('bg-star' + w, 'a-flick', 'animation-duration:' + (2.6 + w * 0.9) + 's', box => { box(x + 21, 11, 1, 1, PAL.b); box(x + 10, 12, 1, 1, PAL.b); }));
    out.push(L('bg-lit' + w, 'a-flick', 'animation-duration:' + (5.2 + w * 1.7) + 's', box => { box(x + 19, 23, 1, 1, PAL.a); box(x + 24, 26, 1, 1, PAL.a); }));
  });
  frames(4, 4, k => L('bg-sec' + k, '', '', box => {
    const sx = k === 0 || k === 1 ? 1 : -1, sy = k === 1 || k === 2 ? 1 : -1;
    for (let i = 1; i <= 3; i++) box(60 + sx * i, 14 + sy * i, 1, 1, PAL.R);
  })).forEach(l => out.push(l));
  out.push(L('bg-note', 'a-flick', 'animation-duration:1.6s', box => box(106, 16, 4, 3, PAL.n)));
  [[202, 32, 'n', 1.1], [206, 37, 'a', 0.7], [202, 42, 'b', 1.9], [200, 47, 'n', 0.5]].forEach((d, k) => out.push(L('bg-led' + k, 'a-flick', 'animation-duration:' + d[3] + 's', box => box(d[0], d[1], 1, 1, PAL[d[2]]))));
  frames(3, 2.1, k => L('bg-bub' + k, '', '', box => {
    if (k === 0) box(354, 37, 1, 1, '#cfe6f5'); else if (k === 1) { box(353, 34, 1, 1, '#cfe6f5'); box(355, 36, 1, 1, '#cfe6f5'); } else { box(354, 31, 1, 1, '#cfe6f5'); box(352, 33, 1, 1, '#cfe6f5'); }
  })).forEach(l => out.push(l));
  [[150, 70, 0], [140, 84, 1.5], [158, 92, 3], [246, 66, 2.2], [236, 80, 4], [254, 90, 0.8]].forEach((m, k) => out.push(L('bg-mote' + k, 'a-mote', 'animation-delay:-' + m[2] + 's', box => box(m[0], m[1], 1, 1, PAL.b))));
  return (MEMO._backdrop = out);
}

/* Bot andando (viewBox 0 0 12 23). frame = null: ciclo animado de 4 quadros; 0..3: quadro parado. */
function runnerLayers(role, frame) {
  const P = botPal(role), still = frame != null, out = [];
  const legs = k => L('legs-' + role + k, '', '', (box, map) => map(0, 18, LEGS[k], P));
  if (still) out.push(legs(frame)); else frames(4, 0.48, legs).forEach(l => out.push(l));
  const lift = still && frame % 2 === 1 ? 'a-up' : '';
  out.push(L('run-body-' + role, still ? lift : 'a-walkbob', '', (box, map) => { map(0, 10, TORSO, P); map(0, 0, HEAD[role], P); map(0, 12, ARM_CARRY, P); map(10, 12, ARM_CARRY, P); }));
  out.push(L('run-folder', still ? lift : 'a-walkbob carry', '', (box, map) => map(2, 12, FOLDER_S)));
  return out;
}

/* Viagem do pacote: derivada só de { active, previous }. Bot da mesa de origem leva a pasta;
   se a origem é a mesa "você" (sem bot), a pasta voa como aviãozinho de papel. */
function tripOf(scene) {
  const a = scene.active, p = scene.previous;
  if (a == null || p == null || a === p || !scene.stations[a] || !scene.stations[p]) return null;
  const from = scene.stations[p], plane = from.role === 'approval';
  const fx = X0 + SLOT * p + (plane ? 14 : 13), tx = X0 + SLOT * a + (plane ? 4 : 13);
  const dist = Math.abs(tx - fx);
  const td = +(plane ? Math.max(1.4, dist / 40) : Math.max(0.9, dist / 46) / 0.46).toFixed(2);
  return {
    id: scene.tripId || 0, kind: plane ? 'plane' : 'walk', fromIndex: p, toIndex: a, td: td,
    arrive: +(plane ? td : td * 0.46).toFixed(2),
    cls: 'trip ' + (plane ? 'plane' : 'walk') + (tx < fx ? ' dir-l' : ' dir-r'),
    style: '--from:' + fx + ';--to:' + tx + ';--td:' + td + 's;animation-timing-function:steps(' + Math.max(1, dist) + ',end)',
    vb: plane ? '0 0 12 6' : '0 0 12 23',
    layers: plane ? [L('plane', '', '', (box, map) => map(0, 0, PLANE))] : runnerLayers(from.role, null)
  };
}

/* Estado da cena -> tudo o que a marcação precisa. opt: { scale, traveling }.
   traveling = false depois que a viagem terminou (quem monta o componente controla esse relógio). */
function buildView(scene, opt) {
  opt = opt || {};
  const trip = opt.traveling === false ? null : tripOf(scene);
  const n = scene.stations.length;
  const stations = scene.stations.map((st, i) => {
    const selected = scene.selected === i, active = scene.active === i;
    const from = !!trip && trip.fromIndex === i, to = !!trip && trip.toIndex === i;
    const c = {
      i: i, selected: selected, folder: active && st.visual !== 'skipped', folderDelay: to ? trip.arrive : 0,
      away: from && trip.kind === 'walk', td: trip ? trip.td : 0, badgeDelay: from ? trip.td : to ? trip.arrive : 0
    };
    const layers = stationLayers(st, c);
    const meta = [st.account, st.model, st.time].filter(Boolean).join(' · ');
    const working = st.visual === 'working' && st.role !== 'approval';
    const label = st.label;
    let aria = label + ': ' + st.stateText.toLowerCase();
    if (st.pass) aria += ', ' + st.pass;
    if (working && st.activity) aria += '. ' + st.activity;
    if (working && meta) aria += '. ' + meta;
    return {
      role: st.role, index: i, cls: 'st v-' + st.visual + (active ? ' is-active' : '') + (selected ? ' is-selected' : ''),
      style: '--x:' + (X0 + SLOT * i), aria: aria, pressed: selected, layers: layers,
      label: label, passText: st.pass || '',
      balloon: working && st.activity ? {
        cls: 'balloon ' + (i === 0 ? 'al-l' : i === n - 1 ? 'al-r' : 'al-c') + (to ? ' a-arrive' : ''),
        style: to ? 'animation-delay:' + trip.arrive + 's' : '', activity: st.activity, meta: meta
      } : null
    };
  });
  return { roomStyle: '--s:' + (opt.scale || 3), stations: stations, trip: trip };
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
  /** Mesa com a pasta (a etapa mais recente) e a de onde ela veio. */
  active: number | null;
  previous: number | null;
  selected: number | null;
  /** Cresce a cada passagem da pasta, para reiniciar a viagem. */
  tripId: number;
}

export interface OfficeLayer { k: string; cls: string; style: string; paths: { f: string; d: string }[] }

export interface OfficeTrip { id: number; kind: "walk" | "plane"; td: number; cls: string; style: string; vb: string; layers: OfficeLayer[] }

export interface OfficeView {
  roomStyle: string;
  stations: {
    role: string;
    index: number;
    cls: string;
    style: string;
    aria: string;
    pressed: boolean;
    layers: OfficeLayer[];
    label: string;
    passText: string;
    balloon: { cls: string; style: string; activity: string; meta: string } | null;
  }[];
  trip: OfficeTrip | null;
}

/** Largura e altura lógicas da sala. */
export const OFFICE_W: number = LW;
export const OFFICE_H: number = LH;
export const officeView: (scene: OfficeScene, opt: { scale: number; traveling: boolean }) => OfficeView = buildView;
export const officeTrip: (scene: OfficeScene) => OfficeTrip | null = tripOf;

/** Cenário fixo da sala como SVG: gerado uma vez só. */
let backdropCache: string | null = null;
export function backdropSvg(): string {
  return (backdropCache ??= layersSvg(backdrop()));
}

/** Camadas como SVG, para {@html} dentro de um <svg>. */
export function layersSvg(layers: OfficeLayer[]): string {
  return layers
    .map((l) => `<g class="${l.cls}" style="${l.style}">${l.paths.map((p) => `<path d="${p.d}" fill="${p.f}"/>`).join("")}</g>`)
    .join("");
}
