import {Sequence} from 'remotion';
import {Canvas} from './components/Canvas';
import {FindScene} from './scenes/FindScene';
import {GitScene} from './scenes/GitScene';
import {IntroScene} from './scenes/IntroScene';
import {OutroScene} from './scenes/OutroScene';
import {ProjectScene} from './scenes/ProjectScene';
import {StatementScene} from './scenes/StatementScene';
import {heroTiming} from './timeline/timing';

export const DirgoHero = () => (
  <Canvas>
    <Sequence from={heroTiming.intro.from} durationInFrames={heroTiming.intro.duration}>
      <IntroScene />
    </Sequence>
    <Sequence from={heroTiming.find.from} durationInFrames={heroTiming.find.duration}>
      <FindScene />
    </Sequence>
    <Sequence from={heroTiming.controlText.from} durationInFrames={heroTiming.controlText.duration}>
      <StatementScene eyebrow="Safe by design" duration={heroTiming.controlText.duration}>
        The choice stays yours.
      </StatementScene>
    </Sequence>
    <Sequence from={heroTiming.git.from} durationInFrames={heroTiming.git.duration}>
      <GitScene />
    </Sequence>
    <Sequence from={heroTiming.projectText.from} durationInFrames={heroTiming.projectText.duration}>
      <StatementScene eyebrow="Dirgo 0.5" duration={heroTiming.projectText.duration}>
        Your project speaks first.
      </StatementScene>
    </Sequence>
    <Sequence from={heroTiming.project.from} durationInFrames={heroTiming.project.duration}>
      <ProjectScene />
    </Sequence>
    <Sequence from={heroTiming.outro.from} durationInFrames={heroTiming.outro.duration}>
      <OutroScene />
    </Sequence>
  </Canvas>
);
