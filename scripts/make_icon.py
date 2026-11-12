"""Desenha o ícone do Lume em PNG com fundo transparente (sem dependências)."""
import math, struct, sys, zlib

SIZE = 1024
MARGIN = 100            # respiro transparente padrão dos ícones do macOS
RADIUS = 186            # raio dos cantos do quadrado
CENTER = SIZE / 2
HALF = SIZE / 2 - MARGIN

BASE = (0x1D, 0x1F, 0x24)
EDGE = (0x33, 0x36, 0x3E)
RING = (0x3A, 0x5F, 0x58)
DOT = (0x7C, 0xCF, 0xBD)


def coverage(distance):
    """Cobertura do pixel a partir da distância (negativa = dentro) até a borda da forma."""
    return max(0.0, min(1.0, 0.5 - distance))


def over(color, alpha, layer, amount):
    """Compõe uma camada opaca sobre a cor atual, limitada à área já coberta."""
    return tuple(c + (l - c) * amount for c, l in zip(color, layer)), alpha


rows = bytearray()
for y in range(SIZE):
    rows.append(0)
    py = y + 0.5 - CENTER
    for x in range(SIZE):
        px = x + 0.5 - CENTER
        qx, qy = abs(px) - (HALF - RADIUS), abs(py) - (HALF - RADIUS)
        box = math.hypot(max(qx, 0), max(qy, 0)) + min(max(qx, qy), 0) - RADIUS
        alpha = coverage(box)
        if alpha == 0.0:
            rows += b"\0\0\0\0"
            continue
        r = math.hypot(px, py)
        color = BASE
        color, _ = over(color, alpha, EDGE, coverage(abs(box + 4) - 4))        # filete interno de 8 px
        color, _ = over(color, alpha, RING, coverage(abs(r - 226) - 27.5))     # anel
        color, _ = over(color, alpha, DOT, coverage(r - 112))                  # ponto de luz
        rows += bytes((round(color[0]), round(color[1]), round(color[2]), round(alpha * 255)))


def chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))


png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", SIZE, SIZE, 8, 6, 0, 0, 0))
png += chunk(b"IDAT", zlib.compress(bytes(rows), 9)) + chunk(b"IEND", b"")
open(sys.argv[1], "wb").write(png)
