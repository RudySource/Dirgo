import {colors} from '../design/tokens';
import {mono, typeScale} from '../design/typography';
import {TerminalCursor} from './TerminalCursor';

type TerminalPromptProps = {
  cwd: string;
  command: string;
  cursor?: boolean;
  cursorPaused?: boolean;
};

export const TerminalPrompt = ({
  cwd,
  command,
  cursor = true,
  cursorPaused = false,
}: TerminalPromptProps) => (
  <div
    style={{
      alignItems: 'baseline',
      display: 'flex',
      fontFamily: mono,
      fontSize: typeScale.terminal,
      fontVariantNumeric: 'tabular-nums',
      letterSpacing: '-0.02em',
      lineHeight: 1.45,
      whiteSpace: 'pre',
    }}
  >
    <span style={{color: colors.textSecondary}}>{cwd}</span>
    <span style={{color: colors.accent, margin: '0 15px'}}>❯</span>
    <span style={{color: colors.text}}>{command}</span>
    {cursor ? <TerminalCursor pause={cursorPaused} /> : null}
  </div>
);
