// 문서 본문 서체(D-86): 고딕(Noto Sans KR, 기본) 또는 명조(Noto Serif KR). 이 브라우저에만 저장하는 보기 설정이다
// (계정 설정·문서 내용은 바꾸지 않는다). 명조를 고른 브라우저에서 문서를 열 때만 명조 서체 파일을 받는다.

export type DocFont = 'sans' | 'serif';

const KEY = 'fvoci.docFont';

/** 저장한 선택. 없거나 읽을 수 없으면(사생활 보호 모드 등) 고딕. */
export function storedDocFont(): DocFont {
  try {
    return window.localStorage.getItem(KEY) === 'serif' ? 'serif' : 'sans';
  } catch {
    return 'sans';
  }
}

/** 지금 화면에 적용된 선택. 저장소를 쓸 수 없는 브라우저에서는 저장한 값과 다를 수 있다(이 화면에만 적용). */
export function appliedDocFont(): DocFont {
  return document.documentElement.dataset.docFont === 'serif' ? 'serif' : 'sans';
}

/** 화면에 적용한다(`:root[data-doc-font='serif']`가 문서 본문 서체를 바꾼다). */
export function applyDocFont(font: DocFont): void {
  if (font === 'serif') document.documentElement.dataset.docFont = 'serif';
  else delete document.documentElement.dataset.docFont;
}

/** 고르고 저장한다. 저장할 수 없어도 지금 화면에는 적용한다. */
export function setDocFont(font: DocFont): void {
  applyDocFont(font);
  try {
    if (font === 'serif') window.localStorage.setItem(KEY, 'serif');
    else window.localStorage.removeItem(KEY);
  } catch {
    // 저장소를 쓸 수 없는 브라우저: 이번 화면에만 적용한다.
  }
}
