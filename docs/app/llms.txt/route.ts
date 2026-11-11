import { agentPages } from '@/lib/agent-pages';

export const revalidate = false;

export async function GET() {
  const lines = [
    '# Superb',
    '',
    'A Rust toolkit for building a DAW. Read in this order: the task index; each flow page',
    '(a whole program) and its generated reference (every signature); the architecture and the',
    'terminology; the primitive contracts; then every example program in full.',
    '/llms-full.txt is all of it in one file.',
    '',
  ];
  for (const page of agentPages()) {
    const description = page.data.description ? `: ${page.data.description}` : '';
    lines.push(`- [${page.data.title}](${page.url})${description}`);
  }
  return new Response(lines.join('\n') + '\n');
}
