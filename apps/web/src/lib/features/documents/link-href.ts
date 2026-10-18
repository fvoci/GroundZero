// 선택 도구 막대의 링크 주소 확인. https·http·mailto만 받고, 주소처럼 보이는 글(example.com/a)은 https://를 붙인다.
// 그 밖(javascript: 등)은 거절한다. 저장되는 링크는 Tiptap Link 확장의 주소 검사도 다시 거친다.
export function normalizeHref(raw: string): string | undefined {
  const value = raw.trim();
  if (value === '') return undefined;
  if (/^(https?:\/\/|mailto:)\S+$/i.test(value)) return value;
  if (/^[\w-]+(\.[\w-]+)*\.[a-z]{2,}(:\d+)?(\/\S*)?$/i.test(value)) return `https://${value}`;
  return undefined;
}
