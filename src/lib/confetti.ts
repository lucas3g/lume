// Confete discreto quando o usuário conclui todas as ações de um cartão.

const COLORS = ["#58b88a", "#d9a441", "#8e99f4", "#a98bf5", "#b89dff"];

export function confetti(host: HTMLElement): void {
  if (matchMedia("(prefers-reduced-motion: reduce)").matches) return;
  const box = host.getBoundingClientRect();
  const dpr = devicePixelRatio || 1;
  const canvas = document.createElement("canvas");
  canvas.className = "confetti";
  canvas.width = box.width * dpr;
  canvas.height = box.height * dpr;
  Object.assign(canvas.style, { position: "absolute", inset: "0", width: "100%", height: "100%", pointerEvents: "none", zIndex: "2" });
  host.appendChild(canvas);
  const ctx = canvas.getContext("2d");
  if (!ctx) return canvas.remove();
  ctx.scale(dpr, dpr);
  const parts = Array.from({ length: 80 }, () => ({
    x: box.width - 40,
    y: 30,
    vx: (Math.random() - 0.7) * 9,
    vy: Math.random() * -7 - 2,
    size: Math.random() * 4 + 2,
    color: COLORS[(Math.random() * COLORS.length) | 0],
    angle: Math.random() * 6,
    spin: (Math.random() - 0.5) * 0.4,
    life: 1,
  }));
  let last = performance.now();
  const frame = (now: number) => {
    const dt = Math.min((now - last) / 16.7, 2);
    last = now;
    ctx.clearRect(0, 0, box.width, box.height);
    let alive = false;
    for (const p of parts) {
      p.vy += 0.28 * dt;
      p.vx *= 0.985;
      p.x += p.vx * dt;
      p.y += p.vy * dt;
      p.angle += p.spin * dt;
      p.life -= 0.009 * dt;
      if (p.life <= 0) continue;
      alive = true;
      ctx.save();
      ctx.globalAlpha = p.life;
      ctx.translate(p.x, p.y);
      ctx.rotate(p.angle);
      ctx.fillStyle = p.color;
      ctx.fillRect(-p.size, -p.size / 2, p.size * 2, p.size);
      ctx.restore();
    }
    if (alive) requestAnimationFrame(frame);
    else canvas.remove();
  };
  requestAnimationFrame(frame);
}
