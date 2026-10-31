// Generated "cover art" so the mockups don't depend on screenshots of the game.
// In the real launcher this slot would hold the Modrinth icon, a user-picked
// screenshot, or this same generator as the fallback.
const THEMES = {
  ruby:   { sky: ["#1a0610", "#5c1530"], sun: "#ff8fa8", far: "#3a0d1f", mid: "#5a1530", top: "#c22a55", dirt: "#2a0a16" },
  meadow: { sky: ["#0d2235", "#3f7a8c"], sun: "#ffe7a3", far: "#24505a", mid: "#2b6b47", top: "#5fbf4a", dirt: "#4a2f1d" },
  desert: { sky: ["#2b1306", "#c7713a"], sun: "#fff0b8", far: "#7a3e1c", mid: "#b8703a", top: "#e8c27a", dirt: "#9a6232" },
  frost:  { sky: ["#071629", "#3d6ea8"], sun: "#e6f3ff", far: "#2a4a70", mid: "#5d86b0", top: "#eef6ff", dirt: "#7d93ad" },
  nether: { sky: ["#1a0503", "#6b1a0c"], sun: "#ffb35c", far: "#4a0f08", mid: "#7d1d10", top: "#d4502a", dirt: "#3a0b06" },
  rubynight: { sky: ["#07040f", "#2c1248"], sun: "#f3e9ff", far: "#1c0f33", mid: "#36184a", top: "#d0316a", dirt: "#1a0b1e" },
  end:    { sky: ["#07040f", "#2c1a4f"], sun: "#e9dcff", far: "#241640", mid: "#d8d29a", top: "#efe9b8", dirt: "#9d9566" },
};

function rng(seed) {
  let s = seed >>> 0 || 1;
  return () => ((s = (s * 1664525 + 1013904223) >>> 0) / 4294967296);
}

function ridge(rand, cols, base, amp, rough) {
  const out = [];
  let h = base;
  for (let i = 0; i < cols; i++) {
    h += (rand() - 0.5) * rough;
    h = Math.max(base - amp, Math.min(base + amp, h));
    out.push(Math.round(h));
  }
  return out;
}

export function coverSVG(seed = 1, themeName = "ruby", w = 64, h = 36) {
  const t = THEMES[themeName] ?? THEMES.ruby;
  const r = rng(seed * 7919);
  const far = ridge(r, w, h * 0.45, 6, 3);
  const mid = ridge(r, w, h * 0.6, 5, 2.4);
  const near = ridge(r, w, h * 0.75, 3, 1.6);
  let px = "";
  const col = (heights, fill, cap) => heights.forEach((y, x) => {
    px += `<rect x="${x}" y="${y}" width="1" height="${h - y}" fill="${fill}"/>`;
    if (cap) px += `<rect x="${x}" y="${y}" width="1" height="1" fill="${cap}"/>`;
  });
  col(far, t.far);
  col(mid, t.mid);
  col(near, t.dirt, t.top);
  const sx = w >= 90 ? Math.floor(w * 0.68 + r() * w * 0.2) : Math.floor(8 + r() * (w - 20));
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${w} ${h}" shape-rendering="crispEdges" preserveAspectRatio="xMidYMax slice">
    <defs><linearGradient id="s" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="${t.sky[0]}"/><stop offset="1" stop-color="${t.sky[1]}"/></linearGradient></defs>
    <rect width="${w}" height="${h}" fill="url(#s)"/>
    <rect x="${sx}" y="${Math.round(h * 0.22)}" width="3" height="3" fill="${t.sun}" opacity=".9"/>
    ${px}</svg>`;
  return `url("data:image/svg+xml,${encodeURIComponent(svg)}")`;
}

export function paintCovers(root = document) {
  root.querySelectorAll("[data-cover]").forEach((el) => {
    const [seed, theme, w, h] = el.dataset.cover.split(",");
    el.style.backgroundImage = coverSVG(Number(seed), theme, w ? Number(w) : undefined, h ? Number(h) : undefined);
  });
}

// Tiny spatial navigation so the mockups can be driven with arrows or a gamepad D-pad.
export function enableNav({ initial } = {}) {
  // An open element with .focus-trap.is-open (menus, dialogs) captures navigation.
  const scope = () => document.querySelector(".focus-trap.is-open") ?? document;
  const all = () => [...scope().querySelectorAll(".focusable")].filter((e) => e.offsetParent !== null);
  let current = (initial && document.querySelector(initial)) || all()[0];
  const button = (name) => document.dispatchEvent(new CustomEvent("mock-button", { detail: name }));
  const focus = (el) => {
    if (!el) return;
    document.querySelectorAll(".is-focused").forEach((e) => e.classList.remove("is-focused"));
    el.classList.add("is-focused");
    current = el;
    el.scrollIntoView?.({ block: "nearest", inline: "nearest", behavior: "smooth" });
    document.dispatchEvent(new CustomEvent("mock-focus", { detail: el }));
  };
  const move = (dir) => {
    if (!current || !scope().contains(current)) return focus(all()[0]);
    const a = current.getBoundingClientRect();
    const ac = { x: a.left + a.width / 2, y: a.top + a.height / 2 };
    let best = null, score = Infinity;
    for (const el of all()) {
      if (el === current) continue;
      const b = el.getBoundingClientRect();
      const dx = b.left + b.width / 2 - ac.x, dy = b.top + b.height / 2 - ac.y;
      const ok = { left: dx < -8, right: dx > 8, up: dy < -8, down: dy > 8 }[dir];
      if (!ok) continue;
      const horiz = dir === "left" || dir === "right";
      const s = (horiz ? Math.abs(dx) : Math.abs(dy)) + (horiz ? Math.abs(dy) : Math.abs(dx)) * 2.5;
      if (s < score) { score = s; best = el; }
    }
    focus(best);
  };
  const press = () => { current.classList.add("is-pressed"); setTimeout(() => current.classList.remove("is-pressed"), 120); current.click(); };
  const keys = { ArrowLeft: "left", ArrowRight: "right", ArrowUp: "up", ArrowDown: "down" };
  addEventListener("keydown", (e) => {
    if (keys[e.key]) { e.preventDefault(); move(keys[e.key]); }
    if (e.key === "Enter") press();
    if (e.key === "Escape" || e.key === "Backspace") button("B");
    if (e.key === "x" || e.key === "X") button("X");
    if (e.key === "y" || e.key === "Y") button("Y");
  });
  let prev = [];
  const poll = () => {
    const pad = [...(navigator.getGamepads?.() ?? [])].find(Boolean);
    if (pad) {
      const b = pad.buttons.map((x) => x.pressed);
      const edge = (i) => b[i] && !prev[i];
      if (edge(12)) move("up"); if (edge(13)) move("down"); if (edge(14)) move("left"); if (edge(15)) move("right");
      if (edge(0)) press();
      if (edge(1)) button("B"); if (edge(2)) button("X"); if (edge(3)) button("Y");
      prev = b;
    }
    requestAnimationFrame(poll);
  };
  requestAnimationFrame(poll);
  focus(current);
  return { focus, current: () => current };
}

export function renderChrome(activeTab, hints, tabs = ["Home", "Library", "Browse", "Settings"]) {
  const top = document.createElement("header");
  top.className = "topbar";
  top.innerHTML = `
    <div class="brand"><img src="./ruby-mark.svg" alt="">Ruby</div>
    <nav class="tabs"><span class="bumper">LB</span>
      ${tabs.map((t) => `<span class="tab ${t === activeTab ? "is-active" : ""}">${t}</span>`).join("")}
      <span class="bumper">RB</span></nav>
    <div class="status">
      <span class="pad"><svg viewBox="0 0 24 24"><path d="M7 8h10c2.5 0 4.4 1.7 5 4.1l.8 3.3c.5 2.1-2 3.5-3.5 2l-2.4-2.4H7.1l-2.4 2.4c-1.5 1.5-4-.1-3.5-2l.8-3.3C2.6 9.7 4.5 8 7 8Zm0 3v3m-1.5-1.5h3M16 11h.01M18.5 13.5h.01"/></svg><span class="battery"></span></span>
      <span>8:42 PM</span>
      <span class="account"><span class="avatar">S</span>Steve_QTech</span>
    </div>`;
  document.body.prepend(top);
  if (hints) setHints(hints);
}

export function setHints(hints) {
  let h = document.querySelector("footer.hints");
  if (!h) {
    h = document.createElement("footer");
    h.className = "hints";
    document.body.append(h);
  }
  h.innerHTML = hints.map(([g, label, cls = ""]) => `<span><span class="glyph ${cls}">${g}</span>${label}</span>`).join("");
}
