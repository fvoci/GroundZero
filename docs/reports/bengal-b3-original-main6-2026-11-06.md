# 뱅3 원본 대조군 main6 결과

측정 소스는 `c108395bf4067a652afb4c012e25de82294ac1d7`, SQL 10파일은 `743a8ab9967caf7aeb73776ea6350939e89f8db9`와 일치한다. D1을 유지하고 H2만 임시 제외한 원본 대조군이며, 종료 후 제품 H2를 복원했다. 후보 채택 결과가 아니다.

명령: `tools/measure-perf.sh screens`. T6, 기준일 2026-11-06, 시간대 Asia/Seoul, 달력 2026-11-01~2026-12-12, 모듈·항목 필터 없음. 소스·설치 SQL 체크섬 일치 PASS.

근거: [설계 승인](https://github.com/fvoci/GroundZero/issues/291#issuecomment-6561634508), [수락 경계 결정](https://github.com/fvoci/GroundZero/issues/291#issuecomment-6658842874). 같은 canonical 도구와 고정 v6 판정기를 사용했다.

## 회차 판정

| 시도 | 측정 구간 UTC                                     | 판정         | 이유                                                              |
| ---- | ------------------------------------------------- | ------------ | ----------------------------------------------------------------- |
| 1    | 2026-11-05T20:42:22.103Z~2026-11-05T20:43:17.782Z | CONTAMINATED | 외부 작업 CPU 11.983% > 10%; 전체 시도를 폐기                     |
| 2    | 2026-11-05T20:52:04.798Z~2026-11-05T20:53:00.448Z | VALID        | 27개 화면 표본과 전체 측정창 감사 PASS; 외부 작업 CPU 최대 8.789% |

첫 시도는 화면 표본 자체의 판정이 통과해도 전체 측정창이 오염되어 27회를 모두 CONTAMINATED로 남겼다. 그 값으로 중앙값이나 후보 수락을 판정하지 않았다. 빌드 겹침 폐기 회차는 두 시도 모두 0이다. 준비 중 외부 CPU 상승은 표본창 밖의 원시에 보존했으며, 준비 완료 전 연속 5분 조건과 실제 측정창을 고정 판정기로 각각 검사했다. 실제 측정 직전 5분의 최대 load는 재시도에서 1.47이었다.

## VALID 3회 중앙값과 범위

| 화면          | API / HTTP 수 | 깊이 | p95 중앙값 [최소~최대] ms | 첫 내용 중앙값 [최소~최대] ms |
| ------------- | ------------: | ---: | ------------------------: | ----------------------------: |
| activity      |       11 / 17 |    4 |     79.806 [79.12~84.132] |             426 [416.6~443.3] |
| board         | 14~15 / 20~21 |    4 |    36.833 [33.618~40.049] |           455.7 [434.6~480.1] |
| calendar      |       10 / 16 |    3 |    82.701 [78.502~87.507] |             417.4 [414~428.6] |
| document-open |       13 / 19 |    4 |    17.595 [16.563~17.665] |           342.8 [342.1~348.9] |
| document-tree |       15 / 21 |    4 |    18.416 [17.829~25.899] |           370.9 [368.5~378.2] |
| gantt         |       14 / 20 |    4 |    33.054 [33.053~35.608] |             453.9 [453~455.7] |
| home          |       11 / 17 |    4 |   80.752 [75.557~100.555] |           442.7 [424.3~477.6] |
| my-tasks      |        7 / 13 |    3 |    16.791 [16.043~19.428] |           302.5 [291.2~304.6] |
| task-detail   |       20 / 26 |    4 |     35.122 [34.17~36.719] |             455.3 [455~479.1] |

## 화면 원시값 — 회차마다 판정

| 시도 | 화면          | 회차 | 판정         |  p95 ms | 첫 내용 ms | API / HTTP 수 | 깊이 |
| ---- | ------------- | ---: | ------------ | ------: | ---------: | ------------: | ---: |
| 1    | activity      |    1 | CONTAMINATED |  77.576 |      414.5 |       11 / 17 |    4 |
| 1    | activity      |    2 | CONTAMINATED |   81.04 |      428.4 |       11 / 17 |    4 |
| 1    | activity      |    3 | CONTAMINATED |  73.237 |      413.8 |       11 / 17 |    4 |
| 1    | board         |    1 | CONTAMINATED |  33.396 |      454.5 |       15 / 21 |    4 |
| 1    | board         |    2 | CONTAMINATED |  35.745 |      425.2 |       14 / 20 |    4 |
| 1    | board         |    3 | CONTAMINATED |  31.843 |      419.2 |       14 / 20 |    4 |
| 1    | calendar      |    1 | CONTAMINATED |   82.33 |      423.5 |       10 / 16 |    3 |
| 1    | calendar      |    2 | CONTAMINATED |  84.718 |      427.7 |       10 / 16 |    3 |
| 1    | calendar      |    3 | CONTAMINATED |  84.286 |      429.1 |       10 / 16 |    3 |
| 1    | document-open |    1 | CONTAMINATED |  18.347 |      350.2 |       13 / 19 |    4 |
| 1    | document-open |    2 | CONTAMINATED |  16.963 |      340.6 |       13 / 19 |    4 |
| 1    | document-open |    3 | CONTAMINATED |  18.741 |      352.4 |       13 / 19 |    4 |
| 1    | document-tree |    1 | CONTAMINATED |  30.218 |      370.9 |       15 / 21 |    4 |
| 1    | document-tree |    2 | CONTAMINATED |  16.912 |      363.1 |       15 / 21 |    4 |
| 1    | document-tree |    3 | CONTAMINATED |  17.901 |      363.8 |       15 / 21 |    4 |
| 1    | gantt         |    1 | CONTAMINATED |   37.41 |      464.7 |       14 / 20 |    4 |
| 1    | gantt         |    2 | CONTAMINATED |   34.92 |      465.4 |       14 / 20 |    4 |
| 1    | gantt         |    3 | CONTAMINATED |  36.106 |      455.9 |       14 / 20 |    4 |
| 1    | home          |    1 | CONTAMINATED | 256.653 |      613.1 |       11 / 17 |    4 |
| 1    | home          |    2 | CONTAMINATED |  84.135 |      453.7 |       11 / 17 |    4 |
| 1    | home          |    3 | CONTAMINATED |  76.757 |      429.6 |       11 / 17 |    4 |
| 1    | my-tasks      |    1 | CONTAMINATED |  19.012 |        300 |        7 / 13 |    3 |
| 1    | my-tasks      |    2 | CONTAMINATED |  19.366 |      301.4 |        7 / 13 |    3 |
| 1    | my-tasks      |    3 | CONTAMINATED |  16.143 |      290.5 |        7 / 13 |    3 |
| 1    | task-detail   |    1 | CONTAMINATED |  37.227 |      470.3 |       20 / 26 |    4 |
| 1    | task-detail   |    2 | CONTAMINATED |  34.771 |      447.2 |       20 / 26 |    4 |
| 1    | task-detail   |    3 | CONTAMINATED |  39.498 |      447.9 |       20 / 26 |    4 |
| 2    | activity      |    1 | VALID        |  84.132 |      443.3 |       11 / 17 |    4 |
| 2    | activity      |    2 | VALID        |   79.12 |      416.6 |       11 / 17 |    4 |
| 2    | activity      |    3 | VALID        |  79.806 |        426 |       11 / 17 |    4 |
| 2    | board         |    1 | VALID        |  36.833 |      455.7 |       15 / 21 |    4 |
| 2    | board         |    2 | VALID        |  40.049 |      480.1 |       14 / 20 |    4 |
| 2    | board         |    3 | VALID        |  33.618 |      434.6 |       14 / 20 |    4 |
| 2    | calendar      |    1 | VALID        |  87.507 |      428.6 |       10 / 16 |    3 |
| 2    | calendar      |    2 | VALID        |  78.502 |        414 |       10 / 16 |    3 |
| 2    | calendar      |    3 | VALID        |  82.701 |      417.4 |       10 / 16 |    3 |
| 2    | document-open |    1 | VALID        |  17.665 |      342.1 |       13 / 19 |    4 |
| 2    | document-open |    2 | VALID        |  17.595 |      342.8 |       13 / 19 |    4 |
| 2    | document-open |    3 | VALID        |  16.563 |      348.9 |       13 / 19 |    4 |
| 2    | document-tree |    1 | VALID        |  18.416 |      370.9 |       15 / 21 |    4 |
| 2    | document-tree |    2 | VALID        |  25.899 |      378.2 |       15 / 21 |    4 |
| 2    | document-tree |    3 | VALID        |  17.829 |      368.5 |       15 / 21 |    4 |
| 2    | gantt         |    1 | VALID        |  33.053 |        453 |       14 / 20 |    4 |
| 2    | gantt         |    2 | VALID        |  33.054 |      453.9 |       14 / 20 |    4 |
| 2    | gantt         |    3 | VALID        |  35.608 |      455.7 |       14 / 20 |    4 |
| 2    | home          |    1 | VALID        | 100.555 |      477.6 |       11 / 17 |    4 |
| 2    | home          |    2 | VALID        |  75.557 |      424.3 |       11 / 17 |    4 |
| 2    | home          |    3 | VALID        |  80.752 |      442.7 |       11 / 17 |    4 |
| 2    | my-tasks      |    1 | VALID        |  16.043 |      304.6 |        7 / 13 |    3 |
| 2    | my-tasks      |    2 | VALID        |  19.428 |      302.5 |        7 / 13 |    3 |
| 2    | my-tasks      |    3 | VALID        |  16.791 |      291.2 |        7 / 13 |    3 |
| 2    | task-detail   |    1 | VALID        |  35.122 |      455.3 |       20 / 26 |    4 |
| 2    | task-detail   |    2 | VALID        |   34.17 |        455 |       20 / 26 |    4 |
| 2    | task-detail   |    3 | VALID        |  36.719 |      479.1 |       20 / 26 |    4 |

## EXPLAIN — 별도 실행을 구분한 회차별 값

정식 함수 호출은 native DB 재생이고 홈 분해의 whole_function은 별도 실행이다. 구성 시간을 합산하거나 두 실행을 같은 값으로 취급하지 않는다. 나머지 구성·기존 도구의 역사 비교 질의까지 JSON에 회차별 판정을 보존했다. 아래 C1/C2 후보는 새로 실행하지 않았다.

| 시도 | 실행 / 항목                                                        | 1회 ms / 판정         | 2회 ms / 판정         | 3회 ms / 판정         |
| ---- | ------------------------------------------------------------------ | --------------------- | --------------------- | --------------------- |
| 1    | native_db / app_home                                               | 75.732 / CONTAMINATED | 79.8 / CONTAMINATED   | 74.82 / CONTAMINATED  |
| 1    | native_db / app_calendar                                           | 75.198 / CONTAMINATED | 75.136 / CONTAMINATED | 74.644 / CONTAMINATED |
| 1    | home_decomposition / active_items                                  | 16.175 / CONTAMINATED | 15.785 / CONTAMINATED | 15.811 / CONTAMINATED |
| 1    | home_decomposition / recent_documents                              | 7.61 / CONTAMINATED   | 7.59 / CONTAMINATED   | 7.962 / CONTAMINATED  |
| 1    | home_decomposition / home component: timezone_validation_unchanged | 31.793 / CONTAMINATED | 32.141 / CONTAMINATED | 32.663 / CONTAMINATED |
| 1    | home_decomposition / all_lists_and_json                            | 35.286 / CONTAMINATED | 34.965 / CONTAMINATED | 35.569 / CONTAMINATED |
| 1    | home_decomposition / home component: whole_function                | 69.819 / CONTAMINATED | 69.572 / CONTAMINATED | 70.768 / CONTAMINATED |
| 2    | native_db / app_home                                               | 74.084 / VALID        | 73.735 / VALID        | 71.275 / VALID        |
| 2    | native_db / app_calendar                                           | 72.084 / VALID        | 75.005 / VALID        | 73.093 / VALID        |
| 2    | home_decomposition / active_items                                  | 15.379 / VALID        | 15.368 / VALID        | 16.192 / VALID        |
| 2    | home_decomposition / recent_documents                              | 7.376 / VALID         | 7.368 / VALID         | 8.548 / VALID         |
| 2    | home_decomposition / home component: timezone_validation_unchanged | 31.301 / VALID        | 31.103 / VALID        | 31.491 / VALID        |
| 2    | home_decomposition / all_lists_and_json                            | 33.688 / VALID        | 34.047 / VALID        | 34.328 / VALID        |
| 2    | home_decomposition / home component: whole_function                | 68.736 / VALID        | 70.172 / VALID        | 68.782 / VALID        |

## 실행 기록과 남은 작업

| 작업                                               | 대상               | 결과                              |
| -------------------------------------------------- | ------------------ | --------------------------------- |
| release 서버·통합/측정 시험 실행 파일·웹 준비 빌드 | 측정 소스          | PASS                              |
| canonical 브라우저 측정 시험                       | 각 시도            | PASS, 각 4개 시험                 |
| SQL 원본·설치 체크섬                               | 기준 SQL 10파일    | PASS                              |
| 원본 대조군                                        | 재시도 9화면 × 3회 | VALID                             |
| 제품 SQL 복원·도구 정리                            | 측정 종료          | PASS, H2 복원·측정 도구 자동 정리 |
| H1·H2 대조군·C1·C2 새 측정                         | 후속 단계          | NOT RUN                           |
| 채택 커밋·추가 #173·묶은 최종 HEAD 검사            | 후속 단계          | NOT RUN                           |
| Daybreak 보안 검토                                 | 코디네이터 배정    | NOT RUN                           |

이 결과는 원본 대조군 하나다. 전후 이득, 대상 밖 요청·깊이 불증가, 묶은 최종 HEAD의 수락은 판정하지 않는다. 코디네이터 인계 지시에 따라 여기서 후속 단계를 시작하지 않는다.
