import { defineConfig } from 'vite';

// PPTX 미리보기 프레임 스크립트(D-74)를 한 파일(IIFE)로 묶는다. 화면은 이 파일의 글을 격리 iframe의 srcdoc에 nonce와 함께
// 넣으므로(프레임은 아무 요청도 하지 않는다) 다른 조각을 불러오면 안 된다. public/에 두어 개발 서버와 운영 빌드가 함께 쓴다.
export default defineConfig({
  plugins: [
    {
      // 화면이 이 글을 <script> 안에 그대로 넣으므로 닫는 태그·주석 시작이 생기면 빌드를 멈춘다.
      name: 'fvoci-inline-safe',
      generateBundle(_options, bundle) {
        for (const file of Object.values(bundle)) {
          if (file.type === 'chunk' && /<\/script|<!--/i.test(file.code)) {
            this.error(`${file.fileName} contains </script or <!-- and cannot be inlined`);
          }
        }
      },
    },
  ],
  publicDir: false,
  define: { 'process.env.NODE_ENV': JSON.stringify('production') },
  build: {
    outDir: 'public',
    emptyOutDir: false,
    copyPublicDir: false,
    lib: {
      entry: 'src/app/pptx-frame.ts',
      formats: ['iife'],
      name: 'fvociPptxFrame',
      fileName: () => 'pptx-frame.js',
    },
  },
});
