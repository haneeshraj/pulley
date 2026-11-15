/*
 * Pulley Figma plugin: main thread.
 *
 * Reads the current selection (read-only, see read.ts) when the UI asks, and keeps the bridge
 * pairing secret in figma.clientStorage (stored on this computer only). The UI does
 * everything else: conversion with pulley-core (WebAssembly) and the connection to the local
 * Pulley bridge. This thread has no network access and changes nothing in the document.
 */
import { readSelection, ReadError } from './read';

const SECRET_KEY = 'pulley.bridge-secret';
const SECRET_RE = /^[0-9a-f]{64}$/;

figma.showUI(__html__, { width: 320, height: 460, themeColors: true, title: 'Pulley' });

function post(msg: Record<string, unknown>): void {
  figma.ui.postMessage(msg);
}

function selectionInfo(): Record<string, unknown> {
  const sel = figma.currentPage.selection;
  return { type: 'selection', count: sel.length, names: sel.slice(0, 3).map((n) => n.name) };
}

function colorProfile(): string {
  try {
    const p = (figma.root as unknown as { documentColorProfile?: string }).documentColorProfile;
    return typeof p === 'string' ? p : 'SRGB';
  } catch (e) {
    return 'SRGB';
  }
}

figma.on('selectionchange', () => post(selectionInfo()));
figma.on('currentpagechange', () => post(selectionInfo()));

figma.ui.onmessage = async (msg: unknown) => {
  if (!msg || typeof msg !== 'object') { return; }
  const m = msg as { type?: unknown; request?: unknown; secret?: unknown; text?: unknown; error?: unknown };
  switch (m.type) {
    case 'ready': {
      let secret: string | null = null;
      try {
        const v = await figma.clientStorage.getAsync(SECRET_KEY);
        if (typeof v === 'string' && SECRET_RE.test(v)) { secret = v; }
      } catch (e) { /* no stored pairing */ }
      post(Object.assign(selectionInfo(), { type: 'init', secret, documentName: figma.root.name }));
      break;
    }
    case 'read': {
      const request = String(m.request || '').slice(0, 64);
      try {
        const json = readSelection(figma.currentPage.selection, { documentName: figma.root.name, colorProfile: colorProfile(), mixed: figma.mixed });
        post({ type: 'nodes', request, json });
      } catch (e) {
        const err = e instanceof ReadError ? e : new ReadError(
          'read_failed',
          'Pulley could not read the selection (' + String((e && (e as Error).message) || e) + ').',
          'Try a smaller selection. If it keeps failing, note which layers are selected and report it.',
        );
        post({ type: 'read_error', request, code: err.code, message: err.message, hint: err.hint });
      }
      break;
    }
    case 'set-secret':
      if (typeof m.secret === 'string' && SECRET_RE.test(m.secret)) { await figma.clientStorage.setAsync(SECRET_KEY, m.secret); }
      break;
    case 'forget-secret':
      await figma.clientStorage.deleteAsync(SECRET_KEY);
      break;
    case 'notify':
      figma.notify(String(m.text || '').slice(0, 140), { error: m.error === true });
      break;
    default:
      break;
  }
};
