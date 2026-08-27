import {directoryCandidates} from '../content/demo';
import {TerminalScene} from './TerminalScene';

export const FindScene = () => (
  <TerminalScene
    cwd="~/dev"
    typedCommand="dgo sl"
    completedCommand=""
    candidates={directoryCandidates}
    title="Directories"
    resultRange="1–4 of 4"
    action="open"
    duration={300}
    selectionFrames={[{at: 0, index: 0}]}
    actionAt={234}
  />
);
