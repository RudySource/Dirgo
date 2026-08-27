import type {CSSProperties, ReactNode} from 'react';
import {useCurrentFrame} from 'remotion';
import {fadeWindow, softReveal} from '../animations/motion';
import {colors} from '../design/tokens';
import {sans} from '../design/typography';

type ProductTextProps = {
  children: ReactNode;
  eyebrow?: string;
  enterAt?: number;
  enterDuration?: number;
  exitAt: number;
  exitDuration?: number;
  align?: 'left' | 'center';
  maxWidth?: number;
  compact?: boolean;
};

export const ProductText = ({
  children,
  eyebrow,
  enterAt = 0,
  enterDuration = 34,
  exitAt,
  exitDuration = 24,
  align = 'center',
  maxWidth = 1500,
  compact = false,
}: ProductTextProps) => {
  const frame = useCurrentFrame();
  const reveal = softReveal(frame, enterAt, enterDuration);
  const opacity = Math.min(
    reveal.opacity,
    fadeWindow(frame, enterAt, enterDuration, exitAt, exitDuration),
  );
  const style: CSSProperties = {
    opacity,
    filter: `blur(${reveal.blur}px)`,
    transform: `translateY(${reveal.translateY}px) scale(${reveal.scale})`,
    transformOrigin: align === 'left' ? 'left center' : 'center',
    textAlign: align,
    maxWidth,
    fontFamily: sans,
  };

  return (
    <div style={style}>
      {eyebrow ? (
        <div
          style={{
            color: colors.accentSoft,
            fontSize: 17,
            fontWeight: 720,
            letterSpacing: '0.16em',
            marginBottom: 28,
            textTransform: 'uppercase',
          }}
        >
          {eyebrow}
        </div>
      ) : null}
      <div
        style={{
          fontSize: compact ? 82 : 104,
          fontWeight: 650,
          letterSpacing: compact ? '-0.048em' : '-0.055em',
          lineHeight: compact ? 0.98 : 0.94,
        }}
      >
        {children}
      </div>
    </div>
  );
};
