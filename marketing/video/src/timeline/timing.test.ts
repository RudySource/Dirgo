import {describe, expect, it} from 'vitest';
import {
  HERO_DURATION_FRAMES,
  LOOP_DURATION_FRAMES,
  FPS,
  sceneProgress,
  typeAtFrame,
} from './timing';

describe('video timing', () => {
  it('keeps the published compositions at the documented durations', () => {
    expect(FPS).toBe(60);
    expect(HERO_DURATION_FRAMES).toBe(1230);
    expect(LOOP_DURATION_FRAMES).toBe(480);
  });

  it('clamps scene progress outside the scene range', () => {
    expect(sceneProgress(9, 10, 20)).toBe(0);
    expect(sceneProgress(20, 10, 20)).toBe(0.5);
    expect(sceneProgress(31, 10, 20)).toBe(1);
  });

  it('types deterministically with a varied cadence', () => {
    const cadence = [2, 3];
    expect(typeAtFrame('dgo', 9, 10, cadence)).toBe('');
    expect(typeAtFrame('dgo', 10, 10, cadence)).toBe('d');
    expect(typeAtFrame('dgo', 12, 10, cadence)).toBe('dg');
    expect(typeAtFrame('dgo', 15, 10, cadence)).toBe('dgo');
    expect(typeAtFrame('dgo', 100, 10, cadence)).toBe('dgo');
  });
});
