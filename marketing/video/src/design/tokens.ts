export const colors = {
  canvas: '#030506',
  canvasLift: '#07100B',
  surface: '#0B0F12',
  surfaceRaised: '#11181D',
  surfaceSelected: '#102018',
  border: '#263038',
  borderStrong: '#35414A',
  text: '#F4F7F8',
  textSecondary: '#8E9AA5',
  textQuiet: '#64717B',
  accent: '#20BF55',
  accentSoft: '#72DF95',
} as const;

export const shadows = {
  terminal: '0 44px 120px rgba(0, 0, 0, 0.55), 0 10px 32px rgba(0, 0, 0, 0.32)',
  selection: '0 0 32px rgba(32, 191, 85, 0.10)',
} as const;

export const radii = {
  terminal: 30,
  panel: 18,
  row: 12,
  pill: 999,
} as const;
