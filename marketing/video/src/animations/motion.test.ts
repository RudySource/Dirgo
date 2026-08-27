import {describe, expect, it} from 'vitest';
import {cameraScale, fadeWindow, softReveal} from './motion';

describe('motion primitives', () => {
  it('fades in, holds, and fades out without exceeding the opacity range', () => {
    expect(fadeWindow(0, 10, 10, 30, 10)).toBe(0);
    expect(fadeWindow(15, 10, 10, 30, 10)).toBeCloseTo(0.5);
    expect(fadeWindow(25, 10, 10, 30, 10)).toBe(1);
    expect(fadeWindow(35, 10, 10, 30, 10)).toBeCloseTo(0.5);
    expect(fadeWindow(50, 10, 10, 30, 10)).toBe(0);
  });

  it('reveals from blur and a small offset with no overshoot', () => {
    expect(softReveal(5, 10, 20)).toEqual({
      opacity: 0,
      blur: 16,
      translateY: 18,
      scale: 0.985,
    });
    expect(softReveal(30, 10, 20)).toEqual({
      opacity: 1,
      blur: 0,
      translateY: 0,
      scale: 1,
    });
  });

  it('caps camera movement at the requested end scale', () => {
    expect(cameraScale(0, 10, 20, 1, 1.03)).toBe(1);
    expect(cameraScale(20, 10, 20, 1, 1.03)).toBeCloseTo(1.015);
    expect(cameraScale(99, 10, 20, 1, 1.03)).toBe(1.03);
  });
});
