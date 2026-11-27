// Run by `nu scripts/design.nu build` through `openpencil eval` (Figma Plugin API), after the token files of
// `design/tokens/<system>/` are imported as variables (a folder is a collection, a file a mode, the system's own file
// the default mode). Every colour, radius and size drawn here is bound to a variable; the only numbers read from the
// tokens are the decoration rules (tick arm, hatch step, dot step), because a drawing needs the value itself.

const collections = figma.variables.getLocalVariableCollections();
const allVars = figma.variables.getLocalVariables();
let inBase;
let coll;
const use = (sys) => {
  coll = collections.find((c) => c.name === sys);
  if (!coll) throw new Error(`no collection ${sys}`);
  inBase = allVars.filter((x) => x.variableCollectionId === coll.id);
};
const v = (name) => {
  const f = inBase.find((x) => x.name === name);
  if (!f) throw new Error(`no variable ${name}`);
  return f;
};
const num = (name) => v(name).valuesByMode[coll.defaultModeId];
const font = (name) => v(name).valuesByMode[coll.defaultModeId];
// A design whose body font is a proportional sans draws its component labels in it; mono is for the ones that are mono.
const bodyMono = () => font('font/body') !== 'Inter';

await figma.loadFontAsync({ family: 'Inter', style: 'Regular' });
let monoOk = true;
try { await figma.loadFontAsync({ family: 'JetBrains Mono', style: 'Regular' }); } catch (e) { monoOk = false; }

const paint = (name) =>
  figma.variables.setBoundVariableForPaint({ type: 'SOLID', color: { r: 0, g: 0, b: 0 } }, 'color', v(name));
const fill = (node, name) => { node.fills = [paint(name)]; };
const stroke = (node, name, w) => { node.strokes = [paint(name)]; node.strokeWeight = w; node.strokeAlign = 'INSIDE'; };
const bind = (node, field, name) => { try { node.setBoundVariable(field, v(name)); } catch (e) {} };

function label(content, size, color, mono) {
  const t = figma.createText();
  const fam = mono && monoOk && bodyMono() ? 'JetBrains Mono' : 'Inter';
  t.fontName = { family: fam, style: 'Regular' };
  t.characters = content;
  t.fontSize = size;
  fill(t, color);
  return t;
}
function box(w, h, fillName, strokeName, radius) {
  const f = figma.createFrame();
  f.resize(w, h);
  f.fills = [];
  if (fillName) fill(f, fillName);
  if (strokeName) stroke(f, strokeName, num('size/border'));
  f.cornerRadius = radius || 0;
  f.clipsContent = true;
  return f;
}
function row(gap) {
  const f = figma.createFrame();
  f.layoutMode = 'HORIZONTAL';
  f.itemSpacing = gap;
  f.primaryAxisSizingMode = 'AUTO';
  f.counterAxisSizingMode = 'AUTO';
  f.fills = [];
  return f;
}
function col(gap) {
  const f = figma.createFrame();
  f.layoutMode = 'VERTICAL';
  f.itemSpacing = gap;
  f.primaryAxisSizingMode = 'AUTO';
  f.counterAxisSizingMode = 'AUTO';
  f.fills = [];
  return f;
}
// A centred text on a frame (a button, a tab, a row).
function caption(parent, content, color, mono) {
  const t = label(content, num('text/body'), color, mono);
  t.x = Math.round((parent.width - t.width) / 2);
  t.y = Math.round((parent.height - t.height) / 2);
  parent.appendChild(t);
  return t;
}
// Four ticks in the corners of a frame, `arm` long.
function ticks(f, arm, colorName) {
  const w = num('size/border');
  const W = f.width, H = f.height;
  const bars = [
    [0, 0, arm, w], [0, 0, w, arm], [W - arm, 0, arm, w], [W - w, 0, w, arm],
    [0, H - w, arm, w], [0, H - arm, w, arm], [W - arm, H - w, arm, w], [W - w, H - arm, w, arm],
  ];
  for (const [x, y, bw, bh] of bars) {
    const r = figma.createRectangle();
    r.resize(bw, bh); r.x = x; r.y = y; fill(r, colorName);
    f.appendChild(r);
  }
}
// 45 degree lines `step` apart over a frame (bottom-left to top-right), in `colorName`, as one vector.
function hatch(f, step, colorName, opacity) {
  const W = f.width, H = f.height;
  let d = '';
  for (let k = 0; k < W + H; k += step) d += `M ${k} 0 L ${k - H} ${H} `;
  const vec = figma.createVector();
  vec.vectorPaths = [{ windingRule: 'NONE', data: d }];
  vec.fills = [];
  vec.strokes = [paint(colorName)];
  vec.strokeWeight = 1;
  vec.opacity = opacity;
  f.appendChild(vec);
}
// A dotted track: solid to `permille`, then a dot every `step`.
function bar(w, permille, step) {
  const h = num('size/track-height');
  const f = box(w, h, null, null, 0);
  const solid = Math.round((w * permille) / 1000);
  if (step > 0) {
    const s = figma.createRectangle();
    s.resize(Math.max(solid, 1), h); fill(s, 'color/primary'); f.appendChild(s);
    for (let x = solid; x < w; x += step) {
      const d = figma.createRectangle();
      d.resize(1, h); d.x = x; fill(d, 'color/border-strong'); f.appendChild(d);
    }
  } else {
    fill(f, 'color/surface-raised');
    const s = figma.createRectangle();
    s.resize(Math.max(solid, 1), h); fill(s, 'color/primary'); f.appendChild(s);
    bind(f, 'cornerRadius', 'radius/sm');
  }
  return f;
}
// A progress round the edge: track all the way, then clockwise from the top left.
function frameProgress(w, h, permille) {
  const t = num('size/track-height');
  const f = box(w, h, null, null, 0);
  const track = figma.createRectangle();
  track.resize(w, h); track.fills = []; stroke(track, 'color/border', t);
  f.appendChild(track);
  let left = ((2 * (w + h) - 4 * t) * permille) / 1000;
  const sides = [[0, 0, 1, 0, w], [w - t, t, 0, 1, h - t], [w - t - 1, h - t, -1, 0, w - t], [0, h - t - 1, 0, -1, h - 2 * t]];
  for (const [sx, sy, dx, dy, len] of sides) {
    if (left <= 0) break;
    const n = Math.min(left, len); left -= n;
    const r = figma.createRectangle();
    const rw = dx ? n : t, rh = dy ? n : t;
    r.resize(Math.max(rw, 1), Math.max(rh, 1));
    r.x = dx === -1 ? sx - n + 1 : sx;
    r.y = dy === -1 ? sy - n + 1 : sy;
    fill(r, 'color/primary');
    f.appendChild(r);
  }
  return f;
}

// The page colour behind a sheet, so the light text of a dark mode reads.
function backdrop(sheet) {
  sheet.paddingLeft = sheet.paddingRight = sheet.paddingTop = sheet.paddingBottom = 32;
  fill(sheet, 'color/background');
}

function foundations(sys) {
  const page = figma.createPage();
  page.name = `${sys} / Foundations`;
  figma.currentPage = page;
  const sheet = col(32);
  sheet.name = 'Foundations';
  backdrop(sheet);
  const roles = inBase.filter((x) => x.name.startsWith('color/')).map((x) => x.name);
  const sw = row(12);
  sw.name = 'Colour roles'; sw.layoutWrap = 'WRAP'; sw.resize(760, 100);
  for (const r of roles) {
    const c = col(6);
    const chip = box(88, 56, r, 'color/border', 0);
    c.appendChild(chip);
    c.appendChild(label(r.replace('color/', ''), 11, 'color/text', true));
    sw.appendChild(c);
  }
  sheet.appendChild(sw);
  const type = col(8);
  type.name = 'Type';
  for (const [role, size, f] of [['title', 'text/title', 'font/title'], ['body', 'text/body', 'font/body'], ['caption', 'text/caption', 'font/body']]) {
    const t = label(`${role} ${num(size)} px ${font(f)} - The quick brown fox 0123456789`, num(size), 'color/text', font(f) !== 'Inter');
    type.appendChild(t);
  }
  sheet.appendChild(type);
  const sp = row(16);
  sp.name = 'Spacing and radius';
  for (const n of ['xs', 'sm', 'md', 'lg']) {
    const c = col(4);
    const b = figma.createRectangle(); b.resize(Math.max(num(`space/${n}`), 1), 24); fill(b, 'color/primary');
    c.appendChild(b); c.appendChild(label(`space/${n} ${num(`space/${n}`)}`, 11, 'color/text', true));
    sp.appendChild(c);
  }
  for (const n of ['sm', 'md', 'lg']) {
    const c = col(4);
    const b = box(56, 56, 'color/surface', 'color/border-strong', num(`radius/${n}`));
    bind(b, 'cornerRadius', `radius/${n}`);
    c.appendChild(b); c.appendChild(label(`radius/${n} ${num(`radius/${n}`)}`, 11, 'color/text', true));
    sp.appendChild(c);
  }
  sheet.appendChild(sp);
  page.appendChild(sheet);
  return page;
}

function controls(sys) {
  const page = figma.createPage();
  page.name = `${sys} / Controls`;
  figma.currentPage = page;
  const ticksArm = num('decor/ticks'), hatchStep = num('decor/hatch'), dots = num('decor/dots');
  const lattice = num('decor/lattice'), seal = num('decor/bar');
  const rad = num('radius/sm');
  const ch = num('size/control-height');
  const sheet = col(32);
  sheet.name = 'Controls';
  backdrop(sheet);

  const group = (title, children) => {
    const g = col(10);
    g.appendChild(label(title, 12, 'color/text-muted', true));
    const r = row(12);
    for (const c of children) r.appendChild(c);
    g.appendChild(r);
    sheet.appendChild(g);
  };

  const button = (text, fillName, strokeName, textName) => {
    const b = box(140, ch, fillName, strokeName, rad);
    bind(b, 'cornerRadius', 'radius/sm');
    caption(b, text, textName, true).name = 'label';
    b.name = `Button / ${text}`;
    return b;
  };
  group('Button', [
    button('Primary', 'color/primary', null, 'color/on-primary'),
    button('Hover', 'color/primary-hover', null, 'color/on-primary'),
    button('Pressed', 'color/primary-active', null, 'color/on-primary'),
    button('Secondary', 'color/surface', 'color/border-strong', 'color/text'),
    button('Disabled', 'color/surface', 'color/border', 'color/text-disabled'),
  ]);

  const item = (text, chosen) => {
    const b = box(220, ch, chosen ? 'color/surface' : null, null, rad);
    if (chosen) {
      if (hatchStep > 0) hatch(b, hatchStep, 'color/primary', 0.62);
      else if (seal > 0) {
        const m = Math.round(ch / 6);
        const bar = figma.createFrame();
        bar.resize(seal, ch - 2 * m);
        bar.x = 0; bar.y = m;
        bar.name = 'Seal bar';
        fill(bar, 'color/primary');
        b.appendChild(bar);
        fill(b, 'color/surface-raised');
      } else fill(b, 'color/primary');
    }
    const t = label(text, num('text/body'), chosen && hatchStep === 0 && seal === 0 ? 'color/on-primary' : 'color/text', true);
    t.x = num('space/md'); t.y = Math.round((ch - t.height) / 2);
    b.appendChild(t);
    b.name = `List item / ${text}`;
    return b;
  };
  group('List item and tab', [item('Chosen', true), item('Idle', false)]);

  const prog = col(12);
  prog.appendChild(bar(320, 500, dots));
  prog.appendChild(bar(320, 1000, dots));
  prog.appendChild(bar(320, 0, dots));
  prog.name = 'Progress bars';
  group('Progress bar', [prog]);
  group('Progress round the frame', [frameProgress(240, 80, 400), frameProgress(240, 80, 1000)]);

  const card = box(260, 110, 'color/surface', 'color/border', num('radius/md'));
  bind(card, 'cornerRadius', 'radius/md');
  if (ticksArm > 0) ticks(card, ticksArm, 'color/border-strong');
  const frameLine = (host, strokeName) => {
    if (lattice === 0) return;
    const f = figma.createFrame();
    f.resize(host.width - 2 * lattice, host.height - 2 * lattice);
    f.x = lattice; f.y = lattice;
    f.name = 'Lattice';
    f.fills = [];
    stroke(f, strokeName, 1);
    host.appendChild(f);
  };
  frameLine(card, 'color/border');
  const t = label('Card', num('text/title'), 'color/text', true); t.x = num('space/md'); t.y = num('space/md'); card.appendChild(t);
  const t2 = label('Ticks in the corners', num('text/caption'), 'color/text-muted', true); t2.x = num('space/md'); t2.y = t.y + t.height + num('space/sm'); card.appendChild(t2);
  card.name = 'Card';
  const dlg = box(260, 110, 'color/surface', 'color/border-strong', num('radius/md'));
  bind(dlg, 'cornerRadius', 'radius/md');
  if (ticksArm > 0) ticks(dlg, ticksArm, 'color/primary');
  frameLine(dlg, 'color/border');
  const d1 = label('Dialog', num('text/title'), 'color/text', true); d1.x = num('space/md'); d1.y = num('space/md'); dlg.appendChild(d1);
  dlg.name = 'Dialog';
  group('Card and dialog', [card, dlg]);
  page.appendChild(sheet);
  return page;
}

// One page with every palette: a row per palette and mode, 16 swatches bound to that palette's variables, the light row
// shown through the palette's light mode.
function palettes() {
  const page = figma.createPage();
  page.name = 'Palettes';
  const sheet = col(10);
  sheet.paddingLeft = sheet.paddingRight = sheet.paddingTop = sheet.paddingBottom = 24;
  sheet.fills = [{ type: 'SOLID', color: { r: 0.12, g: 0.12, b: 0.13 } }];
  sheet.name = 'Palettes';
  for (const c of collections.filter((x) => x.name.startsWith('palette-'))) {
    use(c.name);
    const names = inBase.map((x) => x.name);
    c.modes.forEach((mode, mi) => {
      const r = row(4);
      r.name = 'Palette row';
      const t = figma.createText();
      t.fontName = { family: 'Inter', style: 'Regular' };
      t.characters = `${c.name.slice(8)} ${mi === 0 ? 'dark' : 'light'}`;
      t.fontSize = 12;
      t.fills = [{ type: 'SOLID', color: { r: 0.85, g: 0.85, b: 0.85 } }];
      t.resize(130, t.height);
      r.appendChild(t);
      for (const n of names) {
        const s = figma.createFrame();
        s.resize(26, 26);
        s.name = n;
        fill(s, n);
        try { s.setExplicitVariableModeForCollection(c, mode.modeId); } catch (e) {}
        r.appendChild(s);
      }
      sheet.appendChild(r);
    });
  }
  page.appendChild(sheet);
  return page;
}

const first = figma.currentPage;
palettes();
for (const c of collections.filter((x) => !x.name.startsWith('palette-'))) {
  use(c.name);
  foundations(c.name);
  controls(c.name);
}
if (figma.root.children.length > 1) first.remove();
figma.currentPage = figma.root.children[0];
return `${collections.length} collections, ${figma.root.children.length} pages`;
