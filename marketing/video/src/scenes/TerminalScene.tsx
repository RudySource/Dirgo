import {useCurrentFrame} from 'remotion';
import {cameraScale, fadeWindow, softReveal} from '../animations/motion';
import {Terminal} from '../components/Terminal';
import {TerminalPrompt} from '../components/TerminalPrompt';
import {TerminalResults} from '../components/TerminalResults';
import type {Candidate} from '../content/demo';
import {typeAtFrame} from '../timeline/timing';
import {SceneFrame} from './SceneFrame';

type TerminalSceneProps = {
  cwd: string;
  typedCommand: string;
  completedCommand?: string;
  candidates: readonly Candidate[];
  title: string;
  resultRange: string;
  action: 'open' | 'insert';
  duration: number;
  typingAt?: number;
  selectionFrames: readonly {at: number; index: number}[];
  actionAt: number;
};

const selectedAtFrame = (
  frame: number,
  selectionFrames: readonly {at: number; index: number}[],
): number => {
  let selected = selectionFrames[0]?.index ?? 0;
  for (const selection of selectionFrames) {
    if (frame >= selection.at) selected = selection.index;
  }
  return selected;
};

export const TerminalScene = ({
  cwd,
  typedCommand,
  completedCommand,
  candidates,
  title,
  resultRange,
  action,
  duration,
  typingAt = 42,
  selectionFrames,
  actionAt,
}: TerminalSceneProps) => {
  const frame = useCurrentFrame();
  const reveal = softReveal(frame, 0, 34, {blur: 10, translateY: 30, scale: 0.965});
  const opacity = Math.min(reveal.opacity, fadeWindow(frame, 0, 34, duration - 28, 28));
  const typed = typeAtFrame(typedCommand, frame, typingAt);
  const typingDoneAt = typingAt + typedCommand.length * 3;
  const showResults = frame >= typingDoneAt - 3;
  const selectedIndex = selectedAtFrame(frame, selectionFrames);
  const actionActive = frame >= actionAt && frame < actionAt + 12;
  const command = frame >= actionAt && completedCommand ? completedCommand : typed;
  const scale = cameraScale(frame, 36, Math.max(duration - 80, 1), 0.985, 1.018);

  return (
    <SceneFrame>
      <Terminal
        opacity={opacity}
        blur={reveal.blur}
        scale={scale}
        translateY={reveal.translateY}
      >
        <TerminalPrompt cwd={cwd} command={command} cursorPaused={actionActive} />
        <div
          style={{
            opacity: showResults ? 1 : 0,
            transform: `translateY(${showResults ? 0 : 8}px)`,
          }}
        >
          <TerminalResults
            title={title}
            candidates={candidates}
            selectedIndex={selectedIndex}
            resultRange={resultRange}
            action={action}
            actionActive={actionActive}
          />
        </div>
      </Terminal>
    </SceneFrame>
  );
};
