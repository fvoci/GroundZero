import { readFileSync } from 'node:fs';
import path from 'node:path';

import { describe, expect, it } from 'vitest';

// 밝은·어두운 화면의 색 토큰(app.css의 :root·.dark) 대비가 WCAG 2.2 AA를 넘는지 본다. 글자는 4.5:1, 초점 고리처럼 상태를
// 알리는 표시는 3:1(1.4.11). 반투명 토큰은 그 바탕에 겹쳐 계산한다.
// 예외(사용자 결정 전): 테두리 --border·--input은 두 화면 모두 3:1에 못 미친다(약 1.3~1.5). 입력 칸은 라벨과 함께 보이며,
// 3:1로 올리면 카드·구분선까지 진해져 화면 인상이 바뀌므로 따로 묻는다.

const css = readFileSync(path.resolve(import.meta.dirname, '../src/styles/app.css'), 'utf8');

interface Oklch {
  L: number;
  C: number;
  H: number;
  A: number;
}

function tokens(selector: string): Record<string, Oklch> {
  const block = new RegExp(`^${selector} \\{([\\s\\S]*?)^\\}`, 'm').exec(css)?.[1];
  if (block === undefined) throw new Error(`no ${selector} block in app.css`);
  const out: Record<string, Oklch> = {};
  for (const [, name, value] of block.matchAll(/--([a-z0-9-]+):\s*oklch\(([^)]*)\)/g)) {
    if (name === undefined || value === undefined) continue;
    const [color, alpha] = value.split('/');
    const [L = 0, C = 0, H = 0] = (color ?? '').trim().split(/\s+/).map(Number);
    out[name] = { L, C, H, A: alpha === undefined ? 1 : parseFloat(alpha) / 100 };
  }
  return out;
}

/** OKLCH → 선형 sRGB(0~1로 자른다). */
function linear({ L, C, H }: Oklch): [number, number, number] {
  const h = (H * Math.PI) / 180;
  const a = C * Math.cos(h);
  const b = C * Math.sin(h);
  const l = (L + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m = (L - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s = (L - 0.0894841775 * a - 1.291485548 * b) ** 3;
  const clamp = (x: number): number => Math.min(1, Math.max(0, x));
  return [
    clamp(4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s),
    clamp(-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s),
    clamp(-0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s),
  ];
}

const encode = (x: number): number => (x <= 0.0031308 ? 12.92 * x : 1.055 * x ** (1 / 2.4) - 0.055);
const decode = (x: number): number => (x <= 0.04045 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4);
const luminance = ([r, g, b]: number[]): number =>
  0.2126 * (r ?? 0) + 0.7152 * (g ?? 0) + 0.0722 * (b ?? 0);

function contrast(fg: Oklch, bg: Oklch): number {
  const back = linear(bg);
  // 반투명 글자·선은 화면처럼 감마 공간에서 바탕과 섞는다.
  const front = linear(fg).map((x, i) =>
    decode(encode(x) * fg.A + encode(back[i] ?? 0) * (1 - fg.A)),
  );
  const a = luminance(front);
  const b = luminance(back);
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
}

const TEXT: [string, string][] = [
  ['foreground', 'background'],
  ['card-foreground', 'card'],
  ['popover-foreground', 'popover'],
  ['muted-foreground', 'background'],
  ['muted-foreground', 'muted'],
  ['muted-foreground', 'card'],
  ['muted-foreground', 'popover'],
  ['primary-foreground', 'primary'],
  ['secondary-foreground', 'secondary'],
  ['accent-foreground', 'accent'],
  ['destructive', 'background'],
  ['destructive', 'card'],
  ['sidebar-foreground', 'sidebar'],
  ['sidebar-accent-foreground', 'sidebar-accent'],
  ['sidebar-primary-foreground', 'sidebar-primary'],
];
const STATE: [string, string][] = [
  ['ring', 'background'],
  ['ring', 'muted'],
  ['ring', 'card'],
  ['sidebar-ring', 'sidebar'],
  ['primary', 'background'],
];

describe.each([
  ['light', ':root'],
  ['dark', '\\.dark'],
])('%s theme color contrast', (_name, selector) => {
  const t = tokens(selector);
  const pick = (name: string): Oklch => {
    const value = t[name];
    if (value === undefined) throw new Error(`missing --${name}`);
    return value;
  };

  it.each(TEXT)('text --%s on --%s is at least 4.5:1', (fg, bg) => {
    expect(contrast(pick(fg), pick(bg))).toBeGreaterThanOrEqual(4.5);
  });

  it.each(STATE)('--%s on --%s is at least 3:1', (fg, bg) => {
    expect(contrast(pick(fg), pick(bg))).toBeGreaterThanOrEqual(3);
  });
});
