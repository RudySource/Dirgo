import {useCurrentFrame} from 'remotion';
import {colors} from '../design/tokens';

type TerminalCursorProps = {
  pause?: boolean;
};

export const TerminalCursor = ({pause = false}: TerminalCursorProps) => {
  const frame = useCurrentFrame();
  const visible = pause || frame % 54 < 34;

  return (
    <span
      style={{
        backgroundColor: colors.accent,
        borderRadius: 1,
        display: 'inline-block',
        height: '1.08em',
        marginLeft: 5,
        opacity: visible ? 1 : 0,
        verticalAlign: '-0.12em',
        width: 3,
      }}
    />
  );
};
