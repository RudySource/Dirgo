import {config as remotion} from '@remotion/eslint-config-flat';

export default [
  ...remotion,
  {
    ignores: ['.remotion/**', 'node_modules/**', 'out/**'],
  },
];
