// 사람이 적은 링크를 href로 쓸 때의 확인. http·https만(javascript:·data: 등은 링크로 만들지 않는다).
// 서버의 app_web_url_ok와 같은 기준이다.

export function isWebUrl(value: string): boolean {
  return /^https?:\/\/\S+$/i.test(value);
}
