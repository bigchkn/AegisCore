import { render, screen, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { MarkdownRenderer } from './MarkdownRenderer';

describe('MarkdownRenderer', () => {
  it('renders headings, lists, tables, and code blocks', () => {
    render(
      <MarkdownRenderer
        content={[
          '# Title',
          '',
          'Intro with **bold** and `code`.',
          '',
          '- First',
          '- Second',
          '',
          '| Name | Status |',
          '| ---- | ------ |',
          '| API | Ready |',
          '',
          '```ts',
          'const ok = true;',
          '```',
        ].join('\n')}
      />,
    );

    expect(screen.getByRole('heading', { level: 1, name: 'Title' })).toBeDefined();
    expect(screen.getByText('bold')).toBeDefined();
    expect(screen.getByText('code')).toBeDefined();
    expect(screen.getByRole('list')).toBeDefined();
    expect(screen.getByRole('columnheader', { name: 'Name' })).toBeDefined();
    expect(screen.getByRole('cell', { name: 'Ready' })).toBeDefined();
    expect(screen.getByText('const ok = true;')).toBeDefined();
  });

  it('does not render unsafe links as anchors', () => {
    render(<MarkdownRenderer content="[bad](javascript:alert(1)) and [good](https://example.com)" />);

    expect(screen.queryByRole('link', { name: 'bad' })).toBeNull();
    const link = screen.getByRole('link', { name: 'good' });
    expect(link.getAttribute('href')).toBe('https://example.com');
  });

  it('renders markdown content in the Designs reader as structured text', () => {
    render(<MarkdownRenderer content={'## Section\n\n| Key | Value |\n| --- | ----- |\n| A | B |'} />);

    const table = screen.getByRole('table');
    expect(within(table).getByRole('columnheader', { name: 'Key' })).toBeDefined();
    expect(within(table).getByRole('cell', { name: 'B' })).toBeDefined();
  });
});
