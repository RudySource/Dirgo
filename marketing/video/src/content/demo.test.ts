import {describe, expect, it} from 'vitest';
import {
  directoryCandidates,
  gitCandidates,
  projectCandidates,
} from './demo';

describe('verified Dirgo demo content', () => {
  it('uses directory candidates that explain an ambiguous slash query', () => {
    expect(directoryCandidates.map((candidate) => candidate.label)).toEqual([
      'slash',
      'slash-api',
      'slash-web',
      'slash-docs',
    ]);
    expect(directoryCandidates[0]?.detail).toBe('~/dev/slash');
  });

  it('uses only Git commands and descriptions from the built-in catalog', () => {
    expect(gitCandidates).toEqual([
      {
        label: 'checkout',
        source: 'SUB',
        detail: 'Switch branches or restore files',
      },
      {
        label: 'cherry-pick',
        source: 'SUB',
        detail: 'Apply existing commits',
      },
      {label: 'clone', source: 'SUB', detail: 'Clone a repository'},
      {
        label: 'commit',
        source: 'SUB',
        detail: 'Record changes to the repository',
      },
    ]);
  });

  it('uses the six project scripts declared by the Dirgo demo fixture', () => {
    expect(projectCandidates.map((candidate) => candidate.label)).toEqual([
      'build',
      'dev',
      'format',
      'lint',
      'preview',
      'test',
    ]);
    expect(projectCandidates.every((candidate) => candidate.source === 'PROJ')).toBe(true);
    expect(
      projectCandidates.every(
        (candidate) => candidate.detail === 'package.json script · punk-web',
      ),
    ).toBe(true);
  });
});
