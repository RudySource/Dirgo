import {projectCandidates} from '../content/demo';
import {TerminalScene} from './TerminalScene';

export const ProjectScene = () => (
  <TerminalScene
    cwd="~/dev/punk"
    typedCommand="pnpm run "
    completedCommand="pnpm run dev"
    candidates={projectCandidates}
    title="Project commands"
    resultRange="1–6 of 6"
    action="insert"
    duration={246}
    selectionFrames={[
      {at: 0, index: 0},
      {at: 150, index: 1},
    ]}
    actionAt={190}
  />
);
