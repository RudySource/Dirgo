import {gitCandidates} from '../content/demo';
import {TerminalScene} from './TerminalScene';

export const GitScene = ({duration = 300}: {duration?: number}) => (
  <TerminalScene
    cwd="~/dev/slash"
    typedCommand="git c"
    completedCommand="git commit"
    candidates={gitCandidates}
    title="Git commands"
    resultRange="1–4 of 4"
    action="insert"
    duration={duration}
    selectionFrames={[
      {at: 0, index: 0},
      {at: 128, index: 1},
      {at: 154, index: 2},
      {at: 180, index: 3},
    ]}
    actionAt={226}
  />
);
