import {ProductText} from '../components/ProductText';
import {SceneFrame} from './SceneFrame';

export const IntroScene = () => (
  <SceneFrame>
    <ProductText eyebrow="Dirgo" exitAt={112} exitDuration={30}>
      Stop remembering paths.
    </ProductText>
  </SceneFrame>
);
