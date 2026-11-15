/*
 * Pulley Figma plugin: reads the selection into "pulley-figma-nodes" v1, a plain copy of the
 * properties pulley-core needs (converted by core/pulley-core/src/figma.rs).
 *
 * Read-only: it only reads node properties and never changes the document, so nothing is
 * added to the file or its undo history. Shapes are sent as parameters (rectangle radii,
 * ellipse arcs, polygon/star counts) so pulley-core can rebuild editable paths; vectors send
 * their vectorPaths; boolean operations and text send fillGeometry (Figma's own outlines).
 *
 * The functions take the nodes and `figma.mixed` as arguments so tests can run them on plain
 * objects outside Figma.
 */

export const FORMAT = 'pulley-figma-nodes';
export const VERSION = 1;
/** Same limits as pulley-core (limits.rs): stop early instead of freezing Figma. */
export const MAX_NODES = 100_000;
export const MAX_DEPTH = 48;
const MAX_PAINTS = 32;
const MAX_STOPS = 256;
const MAX_DASHES = 16;

export class ReadError extends Error {
  code: string;
  hint: string;
  constructor(code: string, message: string, hint: string) {
    super(message);
    this.code = code;
    this.hint = hint;
  }
}

export interface ReadOptions {
  documentName: string;
  colorProfile: string;
  /** `figma.mixed`. */
  mixed: symbol;
}

interface Ctx {
  count: number;
  mixed: symbol;
}

type Plain = Record<string, unknown>;
// Figma node objects are read through a loose view: which properties exist depends on the
// node type, and checking with `in` keeps this code independent of typings details.
type AnyNode = Record<string, any>;

function num(v: unknown, fallback = 0): number {
  return typeof v === 'number' && isFinite(v) ? v : fallback;
}

function color(c: AnyNode | undefined, withAlpha: boolean): Plain {
  const o: Plain = { r: num(c && c.r), g: num(c && c.g), b: num(c && c.b) };
  if (withAlpha) { o.a = num(c && c.a, 1); }
  return o;
}

function transform(t: unknown): number[][] {
  if (Array.isArray(t) && t.length === 2 && Array.isArray(t[0]) && Array.isArray(t[1])) {
    return [[num(t[0][0], 1), num(t[0][1]), num(t[0][2])], [num(t[1][0]), num(t[1][1], 1), num(t[1][2])]];
  }
  return [[1, 0, 0], [0, 1, 0]];
}

export function readPaints(p: unknown, mixed: symbol): Plain[] {
  if (p === mixed || !Array.isArray(p)) { return []; }
  return p.slice(0, MAX_PAINTS).map((paint: AnyNode) => {
    const o: Plain = { type: String(paint.type), visible: paint.visible !== false, opacity: num(paint.opacity, 1) };
    if (paint.type === 'SOLID') {
      o.color = color(paint.color, false);
    } else if (typeof paint.type === 'string' && paint.type.indexOf('GRADIENT_') === 0) {
      o.gradientStops = (Array.isArray(paint.gradientStops) ? paint.gradientStops : []).slice(0, MAX_STOPS)
        .map((s: AnyNode) => ({ position: num(s.position), color: color(s.color, true) }));
      o.gradientTransform = transform(paint.gradientTransform);
    }
    return o;
  });
}

function geometry(g: unknown): Plain[] {
  if (!Array.isArray(g)) { return []; }
  return g.map((p: AnyNode) => ({ windingRule: String(p.windingRule || 'NONZERO'), data: String(p.data || '') }));
}

function readText(n: AnyNode, mixed: symbol): Plain {
  let segments: AnyNode[] = [];
  try {
    segments = n.getStyledTextSegments(['fontName', 'fontSize', 'letterSpacing', 'lineHeight', 'fills']);
  } catch (e) {
    segments = [];
  }
  return {
    characters: String(n.characters || ''),
    textAlignHorizontal: String(n.textAlignHorizontal || 'LEFT'),
    textAutoResize: String(n.textAutoResize || 'NONE'),
    segments: segments.map((s) => ({
      start: num(s.start),
      end: num(s.end),
      fontName: { family: String((s.fontName && s.fontName.family) || ''), style: String((s.fontName && s.fontName.style) || '') },
      fontSize: num(s.fontSize, 12),
      letterSpacing: s.letterSpacing && s.letterSpacing !== mixed ? { unit: String(s.letterSpacing.unit), value: num(s.letterSpacing.value) } : undefined,
      lineHeight: s.lineHeight && s.lineHeight !== mixed ? { unit: String(s.lineHeight.unit), value: num(s.lineHeight.value) } : undefined,
      fills: readPaints(s.fills, mixed),
    })),
  };
}

function readNode(n: AnyNode, ctx: Ctx, depth: number): Plain {
  ctx.count++;
  if (ctx.count > MAX_NODES) {
    throw new ReadError('limit_exceeded', 'The selection has more than 100,000 layers.', 'Send a smaller selection.');
  }
  if (depth > MAX_DEPTH) {
    throw new ReadError('too_deep', `The selection is nested more than ${MAX_DEPTH} levels deep.`, 'Flatten some groups or frames and try again.');
  }
  const mixed = ctx.mixed;
  const o: Plain = {
    id: String(n.id),
    name: String(n.name || ''),
    type: String(n.type),
    visible: n.visible !== false,
    transform: transform(n.absoluteTransform),
  };
  if ('opacity' in n) { o.opacity = num(n.opacity, 1); }
  if ('blendMode' in n) { o.blendMode = String(n.blendMode); }
  if (n.isMask) { o.isMask = true; }
  if ('width' in n) { o.width = num(n.width); o.height = num(n.height); }
  if (Array.isArray(n.effects) && n.effects.length) {
    o.effects = n.effects.map((e: AnyNode) => ({ type: String(e.type), visible: e.visible !== false }));
  }
  if ('fills' in n && n.type !== 'TEXT') { o.fills = readPaints(n.fills, mixed); }
  if ('strokes' in n) { o.strokes = readPaints(n.strokes, mixed); }
  if ('strokeWeight' in n) {
    o.strokeWeight = n.strokeWeight === mixed
      // Per-side weights (frames, rectangles): After Effects strokes have one width.
      ? Math.max(num(n.strokeTopWeight), num(n.strokeRightWeight), num(n.strokeBottomWeight), num(n.strokeLeftWeight))
      : num(n.strokeWeight);
  }
  if ('strokeAlign' in n) { o.strokeAlign = String(n.strokeAlign); }
  if ('strokeCap' in n) { o.strokeCap = n.strokeCap === mixed ? 'MIXED' : String(n.strokeCap); }
  if ('strokeJoin' in n) { o.strokeJoin = n.strokeJoin === mixed ? 'MIXED' : String(n.strokeJoin); }
  if ('strokeMiterLimit' in n) { o.strokeMiterLimit = num(n.strokeMiterLimit, 4); }
  if (Array.isArray(n.dashPattern) && n.dashPattern.length) { o.dashPattern = n.dashPattern.slice(0, MAX_DASHES).map((d: unknown) => num(d)); }
  if ('topLeftRadius' in n) {
    o.rectangleCornerRadii = [num(n.topLeftRadius), num(n.topRightRadius), num(n.bottomRightRadius), num(n.bottomLeftRadius)];
  } else if ('cornerRadius' in n && n.cornerRadius !== mixed) {
    o.cornerRadius = num(n.cornerRadius);
  }
  switch (n.type) {
    case 'ELLIPSE':
      if (n.arcData) { o.arcData = { startingAngle: num(n.arcData.startingAngle), endingAngle: num(n.arcData.endingAngle, 2 * Math.PI), innerRadius: num(n.arcData.innerRadius) }; }
      break;
    case 'POLYGON':
      o.pointCount = num(n.pointCount, 3);
      break;
    case 'STAR':
      o.pointCount = num(n.pointCount, 5);
      o.innerRadius = num(n.innerRadius, 0.382);
      break;
    case 'VECTOR':
      o.vectorPaths = geometry(n.vectorPaths);
      break;
    case 'BOOLEAN_OPERATION':
      o.fillGeometry = geometry(n.fillGeometry);
      break;
    case 'TEXT':
      o.fillGeometry = geometry(n.fillGeometry);
      o.text = readText(n, mixed);
      break;
    default:
      break;
  }
  if ('clipsContent' in n) { o.clipsContent = !!n.clipsContent; }
  // Boolean operations are imported from their result (fillGeometry), not their operands.
  if (Array.isArray(n.children) && n.type !== 'BOOLEAN_OPERATION') {
    o.children = n.children.map((c: AnyNode) => readNode(c, ctx, depth + 1));
  }
  return o;
}

/** Union of the nodes' bounding boxes in page coordinates. */
function bounds(nodes: readonly AnyNode[]): Plain {
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  for (const n of nodes) {
    const b = n.absoluteBoundingBox;
    if (!b) { continue; }
    x0 = Math.min(x0, num(b.x)); y0 = Math.min(y0, num(b.y));
    x1 = Math.max(x1, num(b.x) + num(b.width)); y1 = Math.max(y1, num(b.y) + num(b.height));
  }
  if (!isFinite(x0)) { return { x: 0, y: 0, width: 1, height: 1 }; }
  return { x: x0, y: y0, width: Math.max(x1 - x0, 1), height: Math.max(y1 - y0, 1) };
}

/** Read the selection. Returns the JSON text sent to pulley-core. */
export function readSelection(selection: readonly unknown[], opts: ReadOptions): string {
  const nodes = selection as readonly AnyNode[];
  if (!nodes.length) {
    throw new ReadError('empty_selection', 'Nothing is selected in Figma.', 'Select the layers to send, then try again.');
  }
  const ctx: Ctx = { count: 0, mixed: opts.mixed };
  const data = {
    format: FORMAT,
    version: VERSION,
    documentName: opts.documentName,
    colorProfile: opts.colorProfile,
    bounds: bounds(nodes),
    nodes: nodes.map((n) => readNode(n, ctx, 1)),
  };
  return JSON.stringify(data);
}
