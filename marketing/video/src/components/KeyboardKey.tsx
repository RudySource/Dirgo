import type {ReactNode} from 'react';
import {colors} from '../design/tokens';
import {mono} from '../design/typography';

type KeyboardKeyProps = {
  children: ReactNode;
  active?: boolean;
};

export const KeyboardKey = ({children, active = false}: KeyboardKeyProps) => (
  <span
    style={{
      alignItems: 'center',
      backgroundColor: active ? colors.surfaceSelected : colors.surfaceRaised,
      border: `1px solid ${active ? 'rgba(32, 191, 85, 0.46)' : colors.border}`,
      borderRadius: 8,
      color: active ? colors.accentSoft : colors.textSecondary,
      display: 'inline-flex',
      fontFamily: mono,
      fontSize: 14,
      fontWeight: 700,
      height: 31,
      justifyContent: 'center',
      letterSpacing: '0.05em',
      minWidth: 45,
      padding: '0 10px',
      textTransform: 'uppercase',
    }}
  >
    {children}
  </span>
);
