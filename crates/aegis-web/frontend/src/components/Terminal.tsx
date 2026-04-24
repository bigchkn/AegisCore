import { FitAddon } from '@xterm/addon-fit';
import { Terminal as XTerm } from '@xterm/xterm';
import { useEffect, useRef } from 'react';
import '@xterm/xterm/css/xterm.css';

type PaneMessage =
  | { type: 'output'; data: string }
  | { type: 'resize'; cols: number; rows: number };

export function Terminal({ agentId }: { agentId: string }) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const socketRef = useRef<WebSocket | null>(null);

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
    fitAddon.fit();

    const socket = new WebSocket(wsUrl(`/ws/pane/${agentId}`));
    socketRef.current = socket;

    socket.onopen = () => {
      sendResize(socket, terminal.cols, terminal.rows);
    };

    socket.onmessage = (event) => {
      const message = JSON.parse(event.data) as PaneMessage;
      if (message.type === 'output') {
        terminal.write(base64ToBytes(message.data));
      }
    };

    const dataSubscription = terminal.onData((data) => {
      if (socket.readyState === WebSocket.OPEN) {
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
      if (socket.readyState === WebSocket.OPEN) {
        sendResize(socket, terminal.cols, terminal.rows);
      }
    };

    window.addEventListener('resize', onResize);

    return () => {
      window.removeEventListener('resize', onResize);
      dataSubscription.dispose();
      socket.close();
      terminal.dispose();
      socketRef.current = null;
    };
  }, [agentId]);

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
