import type {CSSProperties, ReactNode} from 'react';
import {AbsoluteFill} from 'remotion';
import {colors} from '../design/tokens';
import {sans} from '../design/typography';

type CanvasProps = {
  children: ReactNode;
  glowOpacity?: number;
};

export const Canvas = ({children, glowOpacity = 0.34}: CanvasProps) => {
  const background: CSSProperties = {
    backgroundColor: colors.canvas,
    color: colors.text,
    fontFamily: sans,
    overflow: 'hidden',
  };

  return (
    <AbsoluteFill style={background}>
      <AbsoluteFill
        style={{
          background:
            'radial-gradient(ellipse 56% 46% at 50% 54%, rgba(32, 191, 85, 0.09), rgba(3, 5, 6, 0) 72%)',
          opacity: glowOpacity,
        }}
      />
      {children}
    </AbsoluteFill>
  );
};
