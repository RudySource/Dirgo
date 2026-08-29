import type {CSSProperties, ReactNode} from 'react';
import {colors, radii, shadows} from '../design/tokens';
import {mono} from '../design/typography';

type TerminalProps = {
  children: ReactNode;
  opacity?: number;
  blur?: number;
  scale?: number;
  translateY?: number;
  label?: string;
};

export const Terminal = ({
  children,
  opacity = 1,
  blur = 0,
  scale = 1,
  translateY = 0,
  label = 'LOCAL SESSION',
}: TerminalProps) => {
  const style: CSSProperties = {
    backgroundColor: colors.surface,
    border: `1px solid ${colors.borderStrong}`,
    borderRadius: radii.terminal,
    boxShadow: shadows.terminal,
    filter: `blur(${blur}px)`,
    minHeight: 660,
    opacity,
    overflow: 'hidden',
    transform: `translateY(${translateY}px) scale(${scale})`,
    transformOrigin: 'center center',
    width: 1480,
  };

  return (
    <div style={style}>
      <div
        style={{
          alignItems: 'center',
          borderBottom: `1px solid ${colors.border}`,
          color: colors.textQuiet,
          display: 'flex',
          fontFamily: mono,
          fontSize: 13,
          fontWeight: 720,
          height: 58,
          letterSpacing: '0.13em',
          padding: '0 32px',
        }}
      >
        <span style={{alignItems: 'center', display: 'flex', gap: 12}}>
          <span
            style={{
              backgroundColor: colors.accent,
              borderRadius: '50%',
              boxShadow: '0 0 18px rgba(32, 191, 85, 0.22)',
              height: 7,
              width: 7,
            }}
          />
          DIRGO 0.6
        </span>
        <span style={{marginLeft: 'auto'}}>{label}</span>
      </div>
      <div style={{padding: '42px 52px 48px'}}>{children}</div>
    </div>
  );
};
