export type CandidateSource = 'DIR' | 'SUB' | 'PROJ';

export type Candidate = {
  label: string;
  source?: CandidateSource;
  detail: string;
};

export const directoryCandidates: readonly Candidate[] = [
  {label: 'slash', detail: '~/dev/slash'},
  {label: 'slash-api', detail: '~/dev/slash/services/api'},
  {label: 'slash-web', detail: '~/dev/slash/apps/web'},
  {label: 'slash-docs', detail: '~/dev/slash/docs'},
];

export const gitCandidates: readonly Candidate[] = [
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
];

export const projectCandidates: readonly Candidate[] = [
  'build',
  'dev',
  'format',
  'lint',
  'preview',
  'test',
].map((label) => ({
  label,
  source: 'PROJ' as const,
  detail: 'package.json script · punk-web',
}));
