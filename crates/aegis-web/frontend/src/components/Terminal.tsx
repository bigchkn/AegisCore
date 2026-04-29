import { FitAddon } from '@xterm/addon-fit';
import { Terminal as XTerm } from '@xterm/xterm';
import { useEffect, useRef } from 'react';
import '@xterm/xterm/css/xterm.css';

type PaneMessage =
  | { type: 'output'; data: string }
  | { type: 'resize'; cols: number; rows: number };

export type TerminalStatus = 'connecting' | 'connected' | 'reconnecting' | 'disconnected';

export function Terminal({
  agentId,
  onStatusChange,
}: {
  agentId: string;
  onStatusChange?: (status: TerminalStatus) => void;
}) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const socketRef = useRef<WebSocket | null>(null);
  const reconnectTimerRef = useRef<number | null>(null);

  useEffect(() => {
    if (!containerRef.current) {
      return;
    }

    const terminal = new XTerm({
      cursorBlink: true,
      fontFamily: 'Menlo, Monaco, Consolas, "Liberation Mono", monospace',
      fontSize: 13,
      theme: {
        background: '#101316',
        foreground: '#e8eaed',
        cursor: '#e1b84b',
      },
    });
    const fitAddon = new FitAddon();
    terminal.loadAddon(fitAddon);
    terminal.open(containerRef.current);
    // Defer fit until after the first paint so the container has its actual
    // layout dimensions. Calling fit() synchronously here yields 0×0 in jsdom
    // and an unsized container in the browser, which scrambles xterm's initial
    // render until the next window-resize event corrects it.
    const rafId = requestAnimationFrame(() => {
      if (active) fitAddon.fit();
    });

    let active = true;

    const connect = () => {
      if (!active) {
        return;
      }

      onStatusChange?.(socketRef.current ? 'reconnecting' : 'connecting');
      const socket = new WebSocket(wsUrl(`/ws/pane/${agentId}`));
      socketRef.current = socket;

      socket.onopen = () => {
        if (!active) {
          socket.close();
          return;
        }
        onStatusChange?.('connected');
        sendResize(socket, terminal.cols, terminal.rows);
      };

      socket.onmessage = (event) => {
        const message = JSON.parse(event.data) as PaneMessage;
        if (message.type === 'output') {
          terminal.write(base64ToBytes(message.data));
        }
      };

      socket.onclose = () => {
        if (!active) {
          return;
        }
        onStatusChange?.('reconnecting');
        reconnectTimerRef.current = window.setTimeout(connect, 1000);
      };

      socket.onerror = () => {
        if (!active) {
          return;
        }
        onStatusChange?.('disconnected');
      };
    };

    connect();

    const dataSubscription = terminal.onData((data) => {
      const socket = socketRef.current;
      if (socket && socket.readyState === WebSocket.OPEN) {
        socket.send(
          JSON.stringify({
            type: 'input',
            data: bytesToBase64(new TextEncoder().encode(data)),
          }),
        );
      }
    });

    const onResize = () => {
      fitAddon.fit();
      const socket = socketRef.current;
      if (socket && socket.readyState === WebSocket.OPEN) {
        sendResize(socket, terminal.cols, terminal.rows);
      }
    };

    window.addEventListener('resize', onResize);

    const resizeObserver = new ResizeObserver(() => {
      if (!active) return;
      fitAddon.fit();
      const socket = socketRef.current;
      if (socket && socket.readyState === WebSocket.OPEN) {
        sendResize(socket, terminal.cols, terminal.rows);
      }
    });
    resizeObserver.observe(containerRef.current);

    return () => {
      active = false;
      cancelAnimationFrame(rafId);
      resizeObserver.disconnect();
      window.removeEventListener('resize', onResize);
      dataSubscription.dispose();
      if (reconnectTimerRef.current !== null) {
        window.clearTimeout(reconnectTimerRef.current);
      }
      socketRef.current?.close();
      terminal.dispose();
      socketRef.current = null;
      onStatusChange?.('disconnected');
    };
  }, [agentId, onStatusChange]);

  return <div ref={containerRef} className="terminal-root" />;
}

function sendResize(socket: WebSocket, cols: number, rows: number) {
  socket.send(JSON.stringify({ type: 'resize', cols, rows }));
}

function wsUrl(path: string) {
  const protocol = window.location.protocol === 'https:' ? 'wss' : 'ws';
  return `${protocol}://${window.location.host}${path}`;
}

function base64ToBytes(data: string) {
  const binary = window.atob(data);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  return bytes;
}

function bytesToBase64(bytes: Uint8Array) {
  let binary = '';
  for (const byte of bytes) {
    binary += String.fromCharCode(byte);
  }
  return window.btoa(binary);
}
