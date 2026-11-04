import '@testing-library/jest-dom/vitest';
import { vi } from 'vitest';

// jsdom has no layout engine: stub the few browser APIs the UI relies on.
Element.prototype.scrollIntoView = vi.fn();
Object.defineProperty(window, 'matchMedia', {
  writable: true,
  value: (query: string) => ({
    matches: false,
    media: query,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn()
  })
});
Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: vi.fn(async () => {}) } });

// jsdom has no canvas: a recording 2D context is enough for the editor's drawing code.
export interface FakeCtx {
  calls: string[];
  [k: string]: unknown;
}
function fakeContext(): FakeCtx {
  const calls: string[] = [];
  const rec = (name: string) => (...a: unknown[]) => calls.push(`${name}(${a.filter((x) => typeof x === 'number').join(',')})`);
  return {
    calls,
    fillStyle: '',
    strokeStyle: '',
    lineWidth: 1,
    globalAlpha: 1,
    imageSmoothingEnabled: true,
    fillRect: rec('fillRect'),
    clearRect: rec('clearRect'),
    strokeRect: rec('strokeRect'),
    drawImage: rec('drawImage'),
    putImageData: rec('putImageData'),
    setTransform: rec('setTransform')
  };
}
HTMLCanvasElement.prototype.getContext = function (this: HTMLCanvasElement & { __ctx?: FakeCtx }) {
  this.__ctx ??= fakeContext();
  return this.__ctx;
} as unknown as typeof HTMLCanvasElement.prototype.getContext;
if (typeof globalThis.ImageData === 'undefined') {
  (globalThis as unknown as { ImageData: unknown }).ImageData = class {
    constructor(public data: Uint8ClampedArray, public width: number, public height: number) {}
  };
}
