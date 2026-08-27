import {useCurrentFrame} from 'remotion';
import {softReveal} from '../animations/motion';
import {Logo} from '../components/Logo';
import {colors} from '../design/tokens';
import {mono, sans} from '../design/typography';
import {SceneFrame} from './SceneFrame';

export const OutroScene = () => {
  const frame = useCurrentFrame();
  const reveal = softReveal(frame, 8, 38, {blur: 14, translateY: 18, scale: 0.97});
  return (
    <SceneFrame>
      <div
        style={{
          alignItems: 'center',
          display: 'flex',
          flexDirection: 'column',
          filter: `blur(${reveal.blur}px)`,
          opacity: reveal.opacity,
          transform: `translateY(${reveal.translateY}px) scale(${reveal.scale})`,
        }}
      >
        <Logo width={272} />
        <div
          style={{
            color: colors.text,
            fontFamily: sans,
            fontSize: 66,
            fontWeight: 650,
            letterSpacing: '-0.045em',
            marginTop: 42,
          }}
        >
          Go anywhere. Stay in control.
        </div>
        <div
          style={{
            color: colors.textSecondary,
            fontFamily: mono,
            fontSize: 21,
            marginTop: 38,
          }}
        >
          github.com/RudySource/Dirgo
        </div>
      </div>
    </SceneFrame>
  );
};
