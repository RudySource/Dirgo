import {clamp, sceneProgress} from '../timeline/timing';

export type RevealState = {
  opacity: number;
  blur: number;
  translateY: number;
  scale: number;
};

const smoothstep = (value: number): number => {
  const progress = clamp(value, 0, 1);
  return progress * progress * (3 - 2 * progress);
};

const mix = (from: number, to: number, progress: number): number =>
  from + (to - from) * progress;

export const fadeWindow = (
  frame: number,
  enterStart: number,
  enterDuration: number,
  exitStart: number,
  exitDuration: number,
): number => {
  if (frame < enterStart) {
    return 0;
  }
  if (frame < enterStart + enterDuration) {
    return smoothstep(sceneProgress(frame, enterStart, enterDuration));
  }
  if (frame < exitStart) {
    return 1;
  }
  if (frame < exitStart + exitDuration) {
    return 1 - smoothstep(sceneProgress(frame, exitStart, exitDuration));
  }
  return 0;
};

export const softReveal = (
  frame: number,
  startFrame: number,
  durationFrames: number,
  options: Partial<Pick<RevealState, 'blur' | 'translateY' | 'scale'>> = {},
): RevealState => {
  const progress = smoothstep(sceneProgress(frame, startFrame, durationFrames));
  const blur = options.blur ?? 16;
  const translateY = options.translateY ?? 18;
  const scale = options.scale ?? 0.985;

  return {
    opacity: progress,
    blur: mix(blur, 0, progress),
    translateY: mix(translateY, 0, progress),
    scale: mix(scale, 1, progress),
  };
};

export const cameraScale = (
  frame: number,
  startFrame: number,
  durationFrames: number,
  from: number,
  to: number,
): number =>
  mix(from, to, smoothstep(sceneProgress(frame, startFrame, durationFrames)));
