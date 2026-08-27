import type {ReactNode} from 'react';
import {AbsoluteFill} from 'remotion';

export const SceneFrame = ({children}: {children: ReactNode}) => (
  <AbsoluteFill
    style={{
      alignItems: 'center',
      display: 'flex',
      justifyContent: 'center',
    }}
  >
    {children}
  </AbsoluteFill>
);
