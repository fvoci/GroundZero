# D-86 WOFF2 조건 재현 (#214)

이 도구는 승인 원본을 제품 파일에 덮어쓰지 않고, D-86의 **모든 OpenType 표 바이트 동일** 조건을 검사한다. 기준 커밋과 O-37 원본 SHA-256은 스크립트에 고정했다. 후보 WOFF2와 JSON은 무시되는 `target/font-evidence/woff2-probe/`에만 쓴다. 실패는 그대로 종료 코드 1이며 `head`·`DSIG` 예외나 기대값 정규화를 두지 않는다.

```sh
CARGO_BUILD_JOBS=8 TMPDIR=$HOME/.cache/fvoci-tmp sh /home/kinesis/orca/workspaces/GroundZero/tools/fvoci-slot.sh sh tools/probe-font-woff2.sh
```

설치된 Python과 uv를 사용해 프로젝트의 `target/font-tools/`에만 도구를 설치한다. 판은 `font-tools-requirements.txt`에 고정하고 바이너리 wheel만 받는다. 시스템 패키지·Python 런타임·전역 패키지는 설치하지 않는다. 확인 환경은 CPython 3.14.4, uv 0.13.6, Linux x86_64다. 필요한 wheel이 없는 환경은 실패하며 시스템 설치로 우회하지 않는다.

| 도구                      | 판     | 확인한 라이선스                                                  | 출처                                                                                                                                                           |
| ------------------------- | ------ | ---------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| FontTools (`ttLib.woff2`) | 4.60.1 | MIT, 포함 AGL/AGLFN BSD-3-Clause·cu2qu Apache-2.0·filesystem MIT | [LICENSE](https://github.com/fonttools/fonttools/blob/4.60.1/LICENSE), [LICENSE.external](https://github.com/fonttools/fonttools/blob/4.60.1/LICENSE.external) |
| Brotli (Python wheel)     | 1.2.0  | MIT                                                              | [LICENSE](https://github.com/google/brotli/blob/v1.2.0/LICENSE)                                                                                                |

설치된 두 wheel의 LICENSE와 FontTools의 LICENSE.external도 직접 확인했다. FontTools의 LICENSE.external은 소스 시험용 OFL 서체를 설명하지만 설치된 wheel에는 해당 서체 파일이 없다. 실행 코드의 위 라이선스는 D-60 허용 목록 안이며, 제품 런타임 의존성은 추가하지 않는다. `cargo xtask licenses`는 Cargo·pnpm을 검사하므로 이 Python 도구 원문 확인을 그 검사로 대신하지 않는다.

[WOFF2 §5](https://www.w3.org/TR/WOFF2/#transformedTables)는 `DSIG` 제거와 `head.flags` bit 11 설정을 요구한다. FontTools도 이를 수행하며, 선택적인 glyf/loca/hmtx 변환을 모두 끈 경우에도 `head`가 달라진다. 원본 Noto 파일에는 `DSIG`가 있고, 승인 여섯 파일 모두 bit 11이 꺼져 있다. 따라서 현재의 모든 표 동일 조건으로는 표준을 따르는 변환을 채택할 수 없다. 후보는 배포 승인 산출물이 아니며, 제품 assets·CSS·preload·고지는 바꾸지 않는다.

후속 사용자 결정이 있기 전에는 표 비교를 완화하거나 RFN을 바꾸거나 원본 서체를 추가하지 않는다. `tools/font-contract.test.mjs`와 `xtask`의 새 시험은 아직 구현되지 않은 파생 파일 계약의 실패 시험이다.
