export const FPS = 60;
export const HERO_DURATION_FRAMES = 1230;
export const LOOP_DURATION_FRAMES = 480;

export const heroTiming = {
  intro: {from: 0, duration: 144},
  find: {from: 120, duration: 300},
  controlText: {from: 390, duration: 126},
  git: {from: 486, duration: 300},
  projectText: {from: 756, duration: 126},
  project: {from: 852, duration: 246},
  outro: {from: 1074, duration: 156},
} as const;

export const clamp = (value: number, min: number, max: number): number =>
  Math.min(max, Math.max(min, value));

export const sceneProgress = (
  frame: number,
  startFrame: number,
  durationFrames: number,
): number => {
  if (durationFrames <= 0) {
    return frame >= startFrame ? 1 : 0;
  }

  return clamp((frame - startFrame) / durationFrames, 0, 1);
};

export const typeAtFrame = (
  text: string,
  frame: number,
  startFrame: number,
  cadence: readonly number[] = [3, 2, 3, 3, 2, 4, 2],
): string => {
  if (frame < startFrame || text.length === 0) {
    return '';
  }

  let revealFrame = startFrame;
  let visibleCharacters = 0;
  for (let index = 0; index < text.length; index += 1) {
    if (frame < revealFrame) {
      break;
    }
    visibleCharacters = index + 1;
    revealFrame += cadence[index % cadence.length] ?? 3;
  }

  return text.slice(0, visibleCharacters);
};
