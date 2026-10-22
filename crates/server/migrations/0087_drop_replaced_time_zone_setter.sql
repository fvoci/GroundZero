-- 표시 설정으로 대체된 시간대 저장 함수를 없앤다 (D-92, 코드 전반 감사 C6).
--
-- `app_set_time_zone(text)`(0043)는 계정 시간대만 저장하던 함수다. D-81에서 언어·시간대·주 시작·시각 표시를 함께 정하는
-- `app_set_display_settings`로 대체됐고, 그 뒤로 서버·웹·시험·준비 SQL 어디에서도 부르지 않는다. 과거 개발 버전과의
-- 호환만을 위해 남겨 둔 정의라 D-92에 따라 지운다. 저장된 값(`user_settings`)은 그대로다.
DROP FUNCTION app_set_time_zone(text);
