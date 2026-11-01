// A new variable source is measured under target/ only, never imported by the app.
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { writeFileSync } from 'node:fs';
import { chromium } from '@playwright/test';

const root = new URL('../../../', import.meta.url);
const files = new Map([
  ['/regular.otf', 'target/font-tools-sources/NotoSansKR-Regular.otf'],
  ['/bold.otf', 'target/font-tools-sources/NotoSansKR-Bold.otf'],
  ['/variable.woff2', 'target/font-variable/NotoSansKR-VF.woff2'],
]);
const server = createServer((request, response) => {
  const file = files.get(request.url ?? '');
  if (file === undefined) {
    response.setHeader('content-type', 'text/html');
    response.end('<!doctype html><html lang="ko"><title>Font diagnostic</title></html>');
    return;
  }
  void readFile(new URL(file, root)).then(
    (bytes) => {
      response.setHeader('content-type', file.endsWith('.woff2') ? 'font/woff2' : 'font/otf');
      response.end(bytes);
    },
    () => {
      response.statusCode = 404;
      response.end();
    },
  );
});
await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
const address = server.address();
if (address === null || typeof address === 'string') throw new Error('missing diagnostic origin');
const browser = await chromium.launch();
try {
  const page = await browser.newPage({ deviceScaleFactor: 1 });
  await page.goto(`http://127.0.0.1:${String(address.port)}`);
  const results = await page.evaluate(async () => {
    for (const face of [
      { url: '/regular.otf', weight: '400', family: 'Static' },
      { url: '/bold.otf', weight: '700', family: 'Static' },
      { url: '/variable.woff2', weight: '100 900', family: 'Variable' },
    ]) {
      const font = await new FontFace(face.family, `url(${face.url})`, {
        weight: face.weight,
      }).load();
      document.fonts.add(font);
    }
    const samples = [
      '한국어 문서와 연구·프로젝트 노트 2350',
      'Latin 0123456789 !? → ✓',
      '옛한글 ᄀᆞ ᄒᆞᆫ 龍 漢字',
    ];
    const results = [];
    for (const weight of [400, 700]) {
      for (const size of [16, 32, 64]) {
        const draw = (family: string) => {
          const canvas = document.createElement('canvas');
          canvas.width = 1800;
          canvas.height = 320;
          const ctx = canvas.getContext('2d');
          if (ctx === null) throw new Error('missing canvas');
          ctx.font = `${String(weight)} ${String(size)}px ${family}`;
          const widths = samples.map((sample) => ctx.measureText(sample).width);
          samples.forEach((sample, i) => {
            ctx.fillText(sample, 2, 80 + i * 90);
          });
          return { widths, pixels: ctx.getImageData(0, 0, canvas.width, canvas.height).data };
        };
        const staticFont = draw('Static');
        const variableFont = draw('Variable');
        let differentPixels = 0;
        let inkPixels = 0;
        for (let i = 3; i < staticFont.pixels.length; i += 4) {
          if (staticFont.pixels[i] !== variableFont.pixels[i]) differentPixels += 1;
          if (staticFont.pixels[i] !== 0 || variableFont.pixels[i] !== 0) inkPixels += 1;
        }
        results.push({
          weight,
          size,
          widths: [staticFont.widths, variableFont.widths],
          differentPixels,
          inkPixels,
        });
      }
    }
    return { browser: navigator.userAgent, samples, results };
  });
  writeFileSync(
    new URL('target/font-variable/rendering.json', root),
    `${JSON.stringify(results, null, 2)}\n`,
  );
  console.log(JSON.stringify(results, null, 2));
} finally {
  await browser.close();
  await new Promise<void>((resolve, reject) =>
    server.close((error) => {
      if (error) reject(error);
      else resolve();
    }),
  );
}
