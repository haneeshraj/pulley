// Plain objects shaped like Figma nodes (following the Figma Plugin API documentation; not
// captured from Figma), shared by the reader test and the browser UI test.
import assert from 'node:assert/strict';

export const mixed = Symbol('figma.mixed');
export const opts = { documentName: 'Test file', colorProfile: 'SRGB', mixed };
const at = (x, y) => [[1, 0, x], [0, 1, y]];
export const box = (x, y, width, height) => ({ x, y, width, height });
const solid = (r, g, b, opacity = 1) => ({ type: 'SOLID', visible: true, opacity, color: { r, g, b }, blendMode: 'NORMAL' });

export function rect(id, name, x, y, w, h, extra = {}) {
  return {
    id, name, type: 'RECTANGLE', visible: true, opacity: 1, blendMode: 'NORMAL', isMask: false,
    absoluteTransform: at(x, y), absoluteBoundingBox: box(x, y, w, h), width: w, height: h,
    fills: [solid(1, 0, 0)], strokes: [], strokeWeight: 1, strokeAlign: 'INSIDE', strokeCap: 'NONE', strokeJoin: 'MITER',
    strokeMiterLimit: 4, dashPattern: [], cornerRadius: 0, topLeftRadius: 0, topRightRadius: 0, bottomRightRadius: 0, bottomLeftRadius: 0,
    effects: [], ...extra,
  };
}

function textNode(id, x, y) {
  const characters = 'Hello\nWorld';
  return {
    id, name: 'Title', type: 'TEXT', visible: true, opacity: 1, blendMode: 'NORMAL', isMask: false,
    absoluteTransform: at(x, y), absoluteBoundingBox: box(x, y, 120, 40), width: 120, height: 40,
    fills: mixed, strokes: [], strokeWeight: 0, strokeAlign: 'OUTSIDE', strokeCap: 'NONE', strokeJoin: 'MITER', strokeMiterLimit: 4,
    dashPattern: [], effects: [], characters, textAlignHorizontal: 'CENTER', textAutoResize: 'WIDTH_AND_HEIGHT',
    fillGeometry: [{ windingRule: 'NONZERO', data: 'M 0 0 L 10 0 L 10 10 L 0 10 Z' }],
    getStyledTextSegments(fields) {
      assert.deepEqual(fields, ['fontName', 'fontSize', 'letterSpacing', 'lineHeight', 'fills']);
      return [
        { start: 0, end: 6, fontName: { family: 'Inter', style: 'Bold' }, fontSize: 24, letterSpacing: { unit: 'PERCENT', value: 2 }, lineHeight: { unit: 'AUTO' }, fills: [solid(0, 0, 0)] },
        { start: 6, end: 11, fontName: { family: 'Inter', style: 'Regular' }, fontSize: 16, letterSpacing: { unit: 'PIXELS', value: 0 }, lineHeight: { unit: 'PIXELS', value: 20 }, fills: [solid(0.2, 0.4, 0.8)] },
      ];
    },
  };
}

export function frame() {
  return {
    id: '1:1', name: 'Card', type: 'FRAME', visible: true, opacity: 1, blendMode: 'PASS_THROUGH', isMask: false,
    absoluteTransform: at(100, 50), absoluteBoundingBox: box(100, 50, 400, 300), width: 400, height: 300,
    fills: [solid(1, 1, 1)], strokes: [], strokeWeight: mixed, strokeTopWeight: 1, strokeRightWeight: 3, strokeBottomWeight: 1, strokeLeftWeight: 1,
    strokeAlign: 'INSIDE', strokeCap: 'NONE', strokeJoin: 'MITER', strokeMiterLimit: 4, dashPattern: [],
    cornerRadius: mixed, topLeftRadius: 16, topRightRadius: 16, bottomRightRadius: 0, bottomLeftRadius: 0,
    clipsContent: true, effects: [{ type: 'DROP_SHADOW', visible: true, radius: 4 }],
    children: [
      rect('1:2', 'Button', 120, 260, 120, 32, { cornerRadius: 8, topLeftRadius: 8, topRightRadius: 8, bottomRightRadius: 8, bottomLeftRadius: 8, strokes: [solid(0, 0, 1)], strokeWeight: 2, dashPattern: [4, 2] }),
      {
        id: '1:3', name: 'Avatar', type: 'ELLIPSE', visible: true, opacity: 0.5, blendMode: 'MULTIPLY', isMask: false,
        absoluteTransform: at(120, 70), absoluteBoundingBox: box(120, 70, 64, 64), width: 64, height: 64,
        fills: [{ type: 'GRADIENT_LINEAR', visible: true, opacity: 1, gradientTransform: [[1, 0, 0], [0, 1, 0]],
          gradientStops: [{ position: 0, color: { r: 1, g: 0, b: 0, a: 1 } }, { position: 1, color: { r: 0, g: 0, b: 1, a: 1 } }] }],
        strokes: [], strokeWeight: 0, strokeAlign: 'CENTER', strokeCap: 'NONE', strokeJoin: 'MITER', strokeMiterLimit: 4, dashPattern: [],
        arcData: { startingAngle: 0, endingAngle: 2 * Math.PI, innerRadius: 0 }, effects: [],
      },
      textNode('1:4', 200, 80),
      {
        id: '1:5', name: 'Star', type: 'STAR', visible: true, opacity: 1, blendMode: 'NORMAL', isMask: false,
        absoluteTransform: at(300, 150), absoluteBoundingBox: box(300, 150, 50, 50), width: 50, height: 50,
        fills: [solid(1, 0.8, 0)], strokes: [], strokeWeight: 0, strokeAlign: 'CENTER', strokeCap: 'NONE', strokeJoin: 'MITER',
        strokeMiterLimit: 4, dashPattern: [], cornerRadius: 0, pointCount: 5, innerRadius: 0.4, effects: [],
      },
      {
        id: '1:6', name: 'Arrow', type: 'VECTOR', visible: true, opacity: 1, blendMode: 'NORMAL', isMask: false,
        absoluteTransform: at(380, 60), absoluteBoundingBox: box(380, 60, 40, 20), width: 40, height: 20,
        fills: [], strokes: [solid(0, 0, 0)], strokeWeight: 2, strokeAlign: 'CENTER', strokeCap: mixed, strokeJoin: 'ROUND',
        strokeMiterLimit: 4, dashPattern: [], cornerRadius: 0, effects: [],
        vectorPaths: [{ windingRule: 'NONE', data: 'M 0 10 L 40 10 M 30 0 L 40 10 L 30 20' }],
      },
      {
        id: '1:7', name: 'Union', type: 'BOOLEAN_OPERATION', booleanOperation: 'UNION', visible: true, opacity: 1, blendMode: 'NORMAL', isMask: false,
        absoluteTransform: at(300, 220), absoluteBoundingBox: box(300, 220, 60, 40), width: 60, height: 40,
        fills: [solid(0, 0.6, 0.3)], strokes: [], strokeWeight: 0, strokeAlign: 'CENTER', strokeCap: 'NONE', strokeJoin: 'MITER', strokeMiterLimit: 4, dashPattern: [], effects: [],
        fillGeometry: [{ windingRule: 'NONZERO', data: 'M 0 0 L 60 0 L 60 40 L 0 40 Z' }],
        children: [rect('1:8', 'operand', 300, 220, 30, 40)],
      },
      { id: '1:9', name: 'Hidden', ...rect('1:9', 'Hidden', 0, 0, 10, 10), visible: false },
      { id: '1:10', name: 'Slice', type: 'SLICE', visible: true, absoluteTransform: at(0, 0), absoluteBoundingBox: box(0, 0, 10, 10), width: 10, height: 10 },
    ],
  };
}

