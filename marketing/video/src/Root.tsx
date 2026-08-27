import {Composition} from 'remotion';
import {DirgoHero} from './DirgoHero';
import {DirgoLoop} from './DirgoLoop';
import {FPS, HERO_DURATION_FRAMES, LOOP_DURATION_FRAMES} from './timeline/timing';

export const RemotionRoot = () => (
  <>
    <Composition
      id="DirgoHero"
      component={DirgoHero}
      durationInFrames={HERO_DURATION_FRAMES}
      fps={FPS}
      width={1920}
      height={1080}
    />
    <Composition
      id="DirgoLoop"
      component={DirgoLoop}
      durationInFrames={LOOP_DURATION_FRAMES}
      fps={FPS}
      width={1920}
      height={1080}
    />
  </>
);
