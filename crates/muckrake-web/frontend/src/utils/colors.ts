// Wong colorblind-safe palette
// These colors are distinguishable for deuteranopia, protanopia, and tritanopia
const COLORBLIND_SAFE_PALETTE = [
  '#56b4e9', // Sky blue
  '#e69f00', // Orange
  '#009e73', // Bluish green
  '#cc79a7', // Reddish purple
  '#f0e442', // Yellow
  '#0072b2', // Blue
  '#d55e00', // Vermillion
] as const;

function hashString(str: string): number {
  let hash = 0;
  for (let i = 0; i < str.length; i++) {
    const char = str.charCodeAt(i);
    hash = ((hash << 5) - hash) + char;
    hash = hash & hash;
  }
  return Math.abs(hash);
}

export function getEntityColor(type: string): string {
  const index = hashString(type) % COLORBLIND_SAFE_PALETTE.length;
  return COLORBLIND_SAFE_PALETTE[index];
}
