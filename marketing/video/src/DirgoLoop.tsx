import {Sequence} from 'remotion';
import {Canvas} from './components/Canvas';
import {GitScene} from './scenes/GitScene';
import {StatementScene} from './scenes/StatementScene';

export const DirgoLoop = () => (
  <Canvas>
    <Sequence durationInFrames={102}>
      <StatementScene eyebrow="Dirgo suggestions" duration={102}>
        Type less. Stay in flow.
      </StatementScene>
    </Sequence>
    <Sequence from={78} durationInFrames={336}>
      <GitScene duration={336} />
    </Sequence>
  </Canvas>
);
