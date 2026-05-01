import type { ReactNode } from 'react';

type MarkdownBlock =
  | { kind: 'heading'; level: number; text: string }
  | { kind: 'paragraph'; lines: string[] }
  | { kind: 'code'; language: string | null; lines: string[] }
  | { kind: 'list'; ordered: boolean; items: string[] }
  | { kind: 'quote'; lines: string[] }
  | { kind: 'table'; headers: string[]; rows: string[][] }
  | { kind: 'hr' };

export function MarkdownRenderer({ content }: { content: string }) {
  return (
    <div className="markdown-renderer">
      {parseMarkdown(content).map((block, index) => renderBlock(block, index))}
    </div>
  );
}

function parseMarkdown(content: string): MarkdownBlock[] {
  const lines = content.replace(/\r\n/g, '\n').split('\n');
  const blocks: MarkdownBlock[] = [];
  let index = 0;

  while (index < lines.length) {
    const line = lines[index];
    const trimmed = line.trim();

    if (!trimmed) {
      index += 1;
      continue;
    }

    if (trimmed.startsWith('```')) {
      const language = trimmed.slice(3).trim() || null;
      const codeLines: string[] = [];
      index += 1;
      while (index < lines.length && !lines[index].trim().startsWith('```')) {
        codeLines.push(lines[index]);
        index += 1;
      }
      blocks.push({ kind: 'code', language, lines: codeLines });
      index += index < lines.length ? 1 : 0;
      continue;
    }

    if (/^#{1,6}\s+/.test(trimmed)) {
      const [, hashes, text] = trimmed.match(/^(#{1,6})\s+(.*)$/)!;
      blocks.push({ kind: 'heading', level: hashes.length, text });
      index += 1;
      continue;
    }

    if (/^(-{3,}|\*{3,}|_{3,})$/.test(trimmed)) {
      blocks.push({ kind: 'hr' });
      index += 1;
      continue;
    }

    if (isTableStart(lines, index)) {
      const headers = parseTableCells(lines[index]);
      const rows: string[][] = [];
      index += 2;
      while (index < lines.length && /^\s*\|.*\|\s*$/.test(lines[index])) {
        rows.push(parseTableCells(lines[index]));
        index += 1;
      }
      blocks.push({ kind: 'table', headers, rows });
      continue;
    }

    if (/^>\s?/.test(trimmed)) {
      const quoteLines: string[] = [];
      while (index < lines.length && /^>\s?/.test(lines[index].trim())) {
        quoteLines.push(lines[index].trim().replace(/^>\s?/, ''));
        index += 1;
      }
      blocks.push({ kind: 'quote', lines: quoteLines });
      continue;
    }

    if (/^[-*]\s+/.test(trimmed) || /^\d+\.\s+/.test(trimmed)) {
      const ordered = /^\d+\.\s+/.test(trimmed);
      const marker = ordered ? /^\d+\.\s+/ : /^[-*]\s+/;
      const items: string[] = [];
      while (index < lines.length && marker.test(lines[index].trim())) {
        items.push(lines[index].trim().replace(marker, ''));
        index += 1;
      }
      blocks.push({ kind: 'list', ordered, items });
      continue;
    }

    const paragraph: string[] = [];
    while (index < lines.length && lines[index].trim()) {
      if (startsSpecialBlock(lines, index)) break;
      paragraph.push(lines[index].trim());
      index += 1;
    }
    blocks.push({ kind: 'paragraph', lines: paragraph });
  }

  return blocks;
}

function renderBlock(block: MarkdownBlock, index: number) {
  switch (block.kind) {
    case 'heading': {
      const children = renderInline(block.text);
      if (block.level === 1) return <h1 key={index}>{children}</h1>;
      if (block.level === 2) return <h2 key={index}>{children}</h2>;
      if (block.level === 3) return <h3 key={index}>{children}</h3>;
      if (block.level === 4) return <h4 key={index}>{children}</h4>;
      if (block.level === 5) return <h5 key={index}>{children}</h5>;
      return <h6 key={index}>{children}</h6>;
    }
    case 'paragraph':
      return <p key={index}>{renderInline(block.lines.join(' '))}</p>;
    case 'code':
      return (
        <pre key={index} className="markdown-code">
          {block.language ? <span className="markdown-code-language">{block.language}</span> : null}
          <code>{block.lines.join('\n')}</code>
        </pre>
      );
    case 'list': {
      const ListTag = block.ordered ? 'ol' : 'ul';
      return (
        <ListTag key={index}>
          {block.items.map((item, itemIndex) => (
            <li key={itemIndex}>{renderInline(item)}</li>
          ))}
        </ListTag>
      );
    }
    case 'quote':
      return <blockquote key={index}>{block.lines.map((line) => renderInline(line)).reduce(joinWithBreaks, [])}</blockquote>;
    case 'table':
      return (
        <div key={index} className="markdown-table-wrap">
          <table className="markdown-table">
            <thead>
              <tr>
                {block.headers.map((header, cellIndex) => (
                  <th key={cellIndex}>{renderInline(header)}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {block.rows.map((row, rowIndex) => (
                <tr key={rowIndex}>
                  {block.headers.map((_, cellIndex) => (
                    <td key={cellIndex}>{renderInline(row[cellIndex] ?? '')}</td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      );
    case 'hr':
      return <hr key={index} />;
  }
}

function renderInline(text: string): ReactNode[] {
  const nodes: ReactNode[] = [];
  const pattern = /(`[^`]+`|\*\*[^*]+\*\*|\*[^*]+\*|\[[^\]]+\]\([^)]+\))/g;
  let cursor = 0;
  let match: RegExpExecArray | null;

  while ((match = pattern.exec(text))) {
    if (match.index > cursor) nodes.push(text.slice(cursor, match.index));
    const token = match[0];
    const key = nodes.length;

    if (token.startsWith('`')) {
      nodes.push(<code key={key}>{token.slice(1, -1)}</code>);
    } else if (token.startsWith('**')) {
      nodes.push(<strong key={key}>{renderInline(token.slice(2, -2))}</strong>);
    } else if (token.startsWith('*')) {
      nodes.push(<em key={key}>{renderInline(token.slice(1, -1))}</em>);
    } else {
      const link = token.match(/^\[([^\]]+)\]\(([^)]+)\)$/)!;
      const href = safeHref(link[2]);
      nodes.push(
        href ? (
          <a key={key} href={href}>
            {link[1]}
          </a>
        ) : (
          link[1]
        ),
      );
    }

    cursor = match.index + token.length;
  }

  if (cursor < text.length) nodes.push(text.slice(cursor));
  return nodes;
}

function startsSpecialBlock(lines: string[], index: number) {
  const trimmed = lines[index].trim();
  return (
    trimmed.startsWith('```') ||
    /^#{1,6}\s+/.test(trimmed) ||
    /^>\s?/.test(trimmed) ||
    /^[-*]\s+/.test(trimmed) ||
    /^\d+\.\s+/.test(trimmed) ||
    /^(-{3,}|\*{3,}|_{3,})$/.test(trimmed) ||
    isTableStart(lines, index)
  );
}

function isTableStart(lines: string[], index: number) {
  return (
    index + 1 < lines.length &&
    /^\s*\|.*\|\s*$/.test(lines[index]) &&
    /^\s*\|?\s*:?-{3,}:?\s*(\|\s*:?-{3,}:?\s*)+\|?\s*$/.test(lines[index + 1])
  );
}

function parseTableCells(line: string) {
  return line
    .trim()
    .replace(/^\|/, '')
    .replace(/\|$/, '')
    .split('|')
    .map((cell) => cell.trim());
}

function safeHref(href: string) {
  if (/^(https?:|#|\.{0,2}\/|\/)/.test(href)) return href;
  return null;
}

function joinWithBreaks(acc: ReactNode[], node: ReactNode, index: number) {
  if (index > 0) acc.push(<br key={`br-${index}`} />);
  acc.push(node);
  return acc;
}
