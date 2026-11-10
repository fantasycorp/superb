import { source } from '@/lib/source';

/**
 * Pages an agent should read, in reading order: the task index, each flow's
 * page (a whole program) followed by its generated reference, the rules, the
 * words, the primitive contracts, what is not built, then every example
 * program in full.
 */
const ORDER = [
  'quick-start',
  'flows',
  'flows/session',
  'flows/session.ref',
  'flows/track',
  'flows/track.ref',
  'flows/clip',
  'flows/clip.ref',
  'flows/marker',
  'flows/marker.ref',
  'flows/transport',
  'flows/transport.ref',
  'flows/send',
  'flows/send.ref',
  'flows/midi',
  'flows/midi.ref',
  'flows/record',
  'flows/record.ref',
  'flows/metronome',
  'flows/metronome.ref',
  'flows/export',
  'flows/export.ref',
  'flows/devices',
  'flows/devices.ref',
  'architecture',
  'terminology',
  'primitives',
  'primitives/buffers',
  'primitives/realtime',
  'primitives/transport',
  'primitives/midi',
  'primitives/graph',
  'primitives/dynamics',
  'primitives/mixer',
  'primitives/automation',
  'primitives/analysis',
  'primitives/undo',
  'primitives/session',
  'primitives/plugins',
  'primitives/io',
  'remaining-work',
  'examples',
];

export function agentPages() {
  const pages = source.getPages();
  const bySlug = new Map(pages.map((page) => [page.slugs.join('/'), page]));
  const listed = ORDER.flatMap((slug) => {
    const page = bySlug.get(slug);
    return page ? [page] : [];
  });
  // The example programs are found, not listed: a new file under examples/ joins by itself.
  const examples = pages
    .filter((page) => page.slugs.length === 2 && page.slugs[0] === 'examples')
    .sort((a, b) => a.slugs[1].localeCompare(b.slugs[1]));
  return [...listed, ...examples];
}
