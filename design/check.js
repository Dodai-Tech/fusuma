// Reads the built design file and reports what the Controls pages draw, as JSON: for each system the corner-tick arm of the
// card, the hatch step of the chosen row and the dot step of the progress bar. `scripts/design.nu verify` compares these
// with the `decor/*` tokens, which are made from the numbers the toolkit paints with.
const out = {};
const walk = (n, f) => { f(n); (n.children || []).forEach((c) => walk(c, f)); };
for (const page of figma.root.children) {
  const m = /^(.+) \/ Controls$/.exec(page.name);
  if (!m) continue;
  const r = { arm: 0, hatch: 0, dots: 0 };
  walk(page, (n) => {
    if (n.name === 'Card' && n.children) {
      for (const c of n.children) {
        if (c.type === 'RECTANGLE') r.arm = Math.max(r.arm, c.width, c.height);
      }
      // The text children are not rectangles; a card with no ticks has no rectangles at all, so the arm stays 0.
    }
    if (n.type === 'VECTOR' && n.vectorPaths && n.vectorPaths.length) {
      const ks = [...n.vectorPaths[0].data.matchAll(/M ?(-?\d+(?:\.\d+)?) 0/g)].map((x) => Number(x[1]));
      if (ks.length > 1) r.hatch = ks[1] - ks[0];
    }
    if (n.name === 'Progress bars' && n.children && n.children[0]) {
      const xs = n.children[0].children.filter((c) => c.width === 1).map((c) => c.x);
      if (xs.length > 1) r.dots = xs[1] - xs[0];
    }
  });
  out[m[1]] = r;
}
return JSON.stringify(out);
