# D-86 서체 생성·검증과 원래 차단 조건 재현 (#214)

제품 산출물은 `generate-fonts.sh`로만 만든다. 기준 커밋과 O-37의 승인 여섯 원본 SHA-256은 `font_assets.py`와 notices 검사에 고정한다. 원본이 없는 얕은 체크아웃은 같은 Git 커밋의 raw 주소에서 입력을 받아 해시를 대조한다. Noto 원본은 생성 입력으로만 사용하며 D2Coding은 원본 TTF 그대로다.

```sh
sh tools/generate-fonts.sh           # Noto WOFF2·unicode-range CSS·해시 목록 생성
sh tools/generate-fonts.sh --verify  # 실제 파일의 D-86 경계와 변조 거부 시험
sh tools/generate-fonts.sh --check   # 재생성한 파일과 커밋한 산출물의 바이트 동일성
pnpm --filter @fvoci/web run test tests/font-assets.test.ts
```

설치된 Python과 uv를 사용해 프로젝트의 `target/font-tools/`에만 도구를 설치한다. 판은 `font-tools-requirements.txt`에 고정하고 바이너리 wheel만 받는다. 시스템 패키지·Python 런타임·전역 패키지는 설치하지 않는다. 확인 환경은 CPython 3.14.4, uv 0.13.6, Linux x86_64다. 필요한 wheel이 없는 환경은 실패하며 시스템 설치로 우회하지 않는다.

| 도구                      | 판     | 확인한 라이선스                                                  | 출처                                                                                                                                                           |
| ------------------------- | ------ | ---------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| FontTools (`ttLib.woff2`) | 4.60.1 | MIT, 포함 AGL/AGLFN BSD-3-Clause·cu2qu Apache-2.0·filesystem MIT | [LICENSE](https://github.com/fonttools/fonttools/blob/4.60.1/LICENSE), [LICENSE.external](https://github.com/fonttools/fonttools/blob/4.60.1/LICENSE.external) |
| Brotli (Python wheel)     | 1.2.0  | MIT                                                              | [LICENSE](https://github.com/google/brotli/blob/v1.2.0/LICENSE)                                                                                                |

설치된 두 wheel의 LICENSE와 FontTools의 LICENSE.external도 직접 확인했다. FontTools의 LICENSE.external은 소스 시험용 OFL 서체를 설명하지만 설치된 wheel에는 해당 서체 파일이 없다. 실행 코드의 위 라이선스는 D-60 허용 목록 안이며, 제품 런타임 의존성은 추가하지 않는다. `cargo xtask licenses`는 Cargo·pnpm을 검사하므로 이 Python 도구 원문 확인을 그 검사로 대신하지 않는다.

[WOFF2 §5](https://www.w3.org/TR/WOFF2/#transformedTables)는 `DSIG` 제거와 `head.flags` bit 11 설정을 요구한다. [D-86 정정 근거](https://github.com/fvoci/GroundZero/pull/282#issuecomment-6552039345)에 따라 Noto는 이 둘과 체크섬 재계산을 허용한다. Serif는 그 밖의 모든 원시 표 바이트가 같아야 하며 head는 바이트 8~11과 바이트 16의 bit 11만 달라질 수 있다. Sans는 정확한 cmap 분할·합집합·무겹침·원래 글리프 매핑·이체자 선택자 조합과 name ID 0·13·14 보존을 검사한다. 모든 WOFF2의 메타데이터·private 블록 없음과 체크섬도 검사한다.

첫 조각의 한글은 Python의 EUC-KR에서 B0A1~C8FE를 해독한 KS X 1001 2,350자다. 여기에 라틴·결합 부호·일반 문장 부호·통화·화살표·수학·상자·도형·기타 기호·CJK 문장 부호·호환 자모·전각 기호의 명시된 블록을 더한 뒤 원본 cmap과 교차한다. 범위는 `font_assets.py`의 `COMMON_BLOCKS`가 정하고 둘째 조각은 원본의 정확한 차집합이다. 조각은 굵기마다 둘뿐이다.

`probe-font-woff2.sh`는 정정 전의 모든 표 동일 조건을 재현하는 진단이다. 후보는 `target/font-evidence/woff2-probe/`에만 쓰고 원래 조건대로 실패한다. 제품 검증 명령은 위의 `generate-fonts.sh --verify`다.

가변 서체 비교는 `tools/measure_font_variable.py`와 `apps/web/e2e/measure-font-rendering.ts`로 한다. 새 가변 원본·후보와 크기·렌더링 결과는 `target/font-variable/`에만 두며 제품에 채택하지 않는다. 무거운 생성·측정 명령은 작업 브리프가 정한 호스트 빌드 슬롯에서 실행한다.
