// PPTX 미리보기 프레임(D-74). 부모 화면이 보낸 바이트를 @aiden0z/pptx-renderer로 그린다. 이 문서는 같은 출처가 아닌
// 격리 iframe에서만 열리므로 앱의 쿠키·저장소·화면에 닿지 못하고, CSP가 바깥 요청을 막는다. 슬라이드의 링크는 걷어 낸다.
import { PptxViewer, RECOMMENDED_ZIP_LIMITS } from '@aiden0z/pptx-renderer';

interface Request {
  type: 'pptx';
  bytes: ArrayBuffer;
}

function isRequest(data: unknown): data is Request {
  return (
    typeof data === 'object' &&
    data !== null &&
    'type' in data &&
    data.type === 'pptx' &&
    'bytes' in data &&
    data.bytes instanceof ArrayBuffer
  );
}

function reply(
  message: { type: 'ready' } | { type: 'rendered'; slides: number } | { type: 'failed' },
): void {
  // 부모의 출처를 알 수 없는(격리된) 문서라 대상 출처를 지정하지 못한다. 상태만 보내고 내용은 보내지 않는다.
  window.parent.postMessage(message, '*');
}

let started = false;

// 격리되지 않은 곳(앱과 같은 출처, 예: 주소를 직접 연 경우)에서는 아무것도 그리지 않는다.
const isolated = self.origin === 'null' && window.parent !== window;

window.addEventListener('message', (event: MessageEvent<unknown>) => {
  if (!isolated || event.source !== window.parent || started || !isRequest(event.data)) return;
  started = true;
  const container = document.getElementById('slides');
  if (container === null) return;
  void PptxViewer.open(event.data.bytes, container, {
    zipLimits: RECOMMENDED_ZIP_LIMITS,
    fitMode: 'contain',
    renderMode: 'list',
    lazySlides: false,
    onSlideRendered: (_index, element) => {
      for (const link of element.querySelectorAll('a')) link.removeAttribute('href');
    },
  })
    .then((viewer) => {
      reply({ type: 'rendered', slides: viewer.slideCount });
    })
    .catch(() => {
      reply({ type: 'failed' });
    });
});

// 부모는 이 알림을 받은 뒤에 바이트를 보낸다(로드 이벤트와 모듈 실행 순서에 기대지 않는다).
if (isolated) reply({ type: 'ready' });
