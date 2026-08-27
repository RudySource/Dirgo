import {ProductText} from '../components/ProductText';
import {SceneFrame} from './SceneFrame';

type StatementSceneProps = {
  eyebrow: string;
  children: string;
  duration: number;
};

export const StatementScene = ({eyebrow, children, duration}: StatementSceneProps) => (
  <SceneFrame>
    <ProductText eyebrow={eyebrow} exitAt={duration - 26} exitDuration={26} compact>
      {children}
    </ProductText>
  </SceneFrame>
);
