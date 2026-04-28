import { act, render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { Terminal } from './Terminal';

// Minimal xterm mocks — we only care about fit() timing.
const mockFit = vi.fn();
const mockWrite = vi.fn();
const mockDispose = vi.fn();
const mockOnData = vi.fn(() => ({ dispose: vi.fn() }));

vi.mock('@xterm/xterm', () => ({
  Terminal: vi.fn(() => ({
    loadAddon: vi.fn(),
    open: vi.fn(),
    dispose: mockDispose,
    onData: mockOnData,
    write: mockWrite,
    cols: 80,
    rows: 24,
  })),
}));

vi.mock('@xterm/addon-fit', () => ({
  FitAddon: vi.fn(() => ({ fit: mockFit })),
}));

vi.mock('@xterm/xterm/css/xterm.css', () => ({}));

const stubWebSocket = () =>
  vi.stubGlobal(
    'WebSocket',
    class {
      onopen: (() => void) | null = null;
      onclose: (() => void) | null = null;
      onerror: (() => void) | null = null;
      onmessage: ((e: { data: string }) => void) | null = null;
      readyState = 1; // OPEN
      send = vi.fn();
      close = vi.fn(() => this.onclose?.());
    } as any,
  );

describe('Terminal', () => {
  beforeEach(() => {
    mockFit.mockClear();
    mockWrite.mockClear();
    mockDispose.mockClear();
    mockOnData.mockClear();
    stubWebSocket();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.useRealTimers();
  });

  it('defers initial fit until after the first animation frame', async () => {
    vi.useFakeTimers();

    // Render inside act so the useEffect fires synchronously.
    act(() => {
      render(<Terminal agentId="agent-1" />);
    });

    // fit() must NOT have been called yet — the container has no layout
    // dimensions at this point (jsdom reports 0x0), so calling fit() here
    // is what causes scrambled text when the pane is opened/reopened.
    expect(mockFit).not.toHaveBeenCalled();

    // Advance past the requestAnimationFrame.
    await act(async () => {
      vi.runAllTimers();
    });

    expect(mockFit).toHaveBeenCalledTimes(1);
  });

  it('creates a fresh terminal when agentId changes (pane reopen)', async () => {
    vi.useFakeTimers();

    let rerender: (ui: React.ReactElement) => void;

    act(() => {
      ({ rerender } = render(<Terminal agentId="agent-1" />));
    });

    // Flush the first rAF so the first terminal is fully initialised.
    await act(async () => {
      vi.runAllTimers();
    });

    const firstFitCalls = mockFit.mock.calls.length;
    expect(firstFitCalls).toBe(1);

    // Switch to a different agent — this simulates the user reopening the pane
    // for another agent (or re-navigating to the same pane after detaching).
    act(() => {
      rerender(<Terminal agentId="agent-2" />);
    });

    // fit() must not fire synchronously for the new terminal either.
    expect(mockFit).toHaveBeenCalledTimes(firstFitCalls);

    await act(async () => {
      vi.runAllTimers();
    });

    // Exactly one more fit() call for the new terminal.
    expect(mockFit).toHaveBeenCalledTimes(firstFitCalls + 1);
  });
});
