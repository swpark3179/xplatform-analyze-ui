use quick_xml::events::Event;
use quick_xml::reader::Reader;
use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;

use crate::models::{ExtractedAction, ExtractedCombo};

static CDATA_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)<!\[CDATA\[(.*?)\]\]>").expect("CDATA 정규식 컴파일 실패"));

static COMBO_FN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(getComCodeCombo|getComCodeComboSync|getGridCodeCombo|getGridCodeComboSync)\s*\(")
        .expect("COMBO_FN 정규식 컴파일 실패")
});

static SECOND_ARG_STR_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#",\s*(\"([^\"]*)\"|'([^']*)')"#).expect("SECOND_ARG_STR 정규식 컴파일 실패")
});

static ARRAY_SECOND_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"\[\s*[^,\]]+\s*,\s*(\"([^\"]*)\"|'([^']*)')\s*\]"#)
        .expect("ARRAY_SECOND 정규식 컴파일 실패")
});

static ACTION_CALL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"actionSubmit(?:FR)?\s*\(\s*("([^"]+)"|'([^']+)'|([A-Za-z_$][A-Za-z0-9_$]*))"#)
        .expect("ACTION_CALL 정규식 컴파일 실패")
});

/// XFDL 파일을 파싱하여 actionSubmit/actionSubmitFR 호출 목록과 dsAction URL 맵을 반환합니다.
pub fn parse_xfdl(
    xfdl_path: &str,
    xfdl_name: &str,
) -> Result<(Vec<ExtractedAction>, Option<String>), String> {
    let content = std::fs::read_to_string(xfdl_path)
        .map_err(|e| format!("파일 읽기 실패: {e}"))?;

    // 1) dsAction Dataset에서 ID → URL 맵 추출 (에러가 나더라도 중간까지 파싱된 정보 반환)
    let (url_map, parse_err) = extract_dsaction_urls(&content);

    // 2) Script CDATA에서 actionSubmit 호출 추출
    let calls = extract_action_calls(&content)?;

    // 3) 두 결과 합산
    let mut results = Vec::new();
    for (raw_action_id, is_literal) in calls {
        let url = url_map.get(&raw_action_id).cloned();
        let (result_id, action_id, is_manual) = if is_literal {
            (
                format!("{}.{}", xfdl_name, raw_action_id),
                raw_action_id.clone(),
                false,
            )
        } else {
            // 변수인 경우: result_id를 임시 값으로, 수동확인 플래그
            (
                format!("{}.??{}", xfdl_name, raw_action_id),
                raw_action_id.clone(),
                true,
            )
        };

        results.push(ExtractedAction {
            result_id,
            action_id,
            xfdl_path: xfdl_path.to_string(),
            xfdl_name: xfdl_name.to_string(),
            url,
            is_manual,
            xml_parse_err: parse_err.clone(),
        });
    }

    Ok((results, parse_err))
}

/// Script CDATA 내에서 getComCodeCombo / getGridCodeCombo 계열 호출을 추출합니다.
/// 파라미터 1개당 ExtractedCombo 1건 (실제 파라미터는 \| 앞까지, 9자 이하는 공통코드).
pub fn extract_combo_calls(
    content: &str,
    xfdl_path: &str,
    xfdl_name: &str,
) -> Result<Vec<ExtractedCombo>, String> {
    let mut combos = Vec::new();
    let mut combo_index = 0usize;

    for cdata_cap in CDATA_RE.captures_iter(content) {
        let script = &cdata_cap[1];
        let mut pos = 0usize;
        while pos < script.len() {
            let rest = &script[pos..];
            let Some(fn_cap) = COMBO_FN_RE.captures(rest) else {
                break;
            };
            let fn_end = fn_cap.get(0).unwrap().end();
            pos += fn_end;

            // 괄호 짝 맞춰서 인자 영역 끝 찾기
            let args_start = pos;
            let mut depth = 1usize;
            let mut i = pos;
            let script_bytes = script.as_bytes();
            while i < script.len() && depth > 0 {
                let c = script_bytes[i] as char;
                if c == '(' {
                    depth += 1;
                } else if c == ')' {
                    depth -= 1;
                }
                i += 1;
            }
            let args_end = if depth == 0 { i - 1 } else { script.len() };
            let args = script[args_start..args_end].trim();
            pos = args_end + 1;

            let mut params: Vec<String> = Vec::new();
            if args.starts_with('[') {
                // 형태 B: 인자가 배열(들)
                for cap in ARRAY_SECOND_RE.captures_iter(args) {
                    let s = cap.get(2).or_else(|| cap.get(3)).map(|m| m.as_str().to_string());
                    if let Some(p) = s {
                        params.push(p);
                    }
                }
            } else {
                // 형태 A: 두 번째 인자가 문자열
                if let Some(cap) = SECOND_ARG_STR_RE.captures(args) {
                    let s = cap.get(2).or_else(|| cap.get(3)).map(|m| m.as_str().to_string());
                    if let Some(p) = s {
                        params.push(p);
                    }
                }
            }

            for param in params {
                let actual = if let Some(bar) = param.find('|') {
                    param[..bar].trim().to_string()
                } else {
                    param.trim().to_string()
                };
                if actual.is_empty() {
                    continue;
                }
                let is_common_code = actual.len() <= 9;
                combo_index += 1;
                let safe_param = actual.replace(|c: char| !c.is_alphanumeric(), "_");
                let result_id = format!("{}.combo_{}_{}", xfdl_name, combo_index, safe_param);
                combos.push(ExtractedCombo {
                    result_id,
                    xfdl_path: xfdl_path.to_string(),
                    xfdl_name: xfdl_name.to_string(),
                    param: actual.clone(),
                    is_common_code,
                });
            }
        }
    }

    Ok(combos)
}

/// <Dataset id="dsAction"> 내의 <Row>에서 ID → URL 매핑을 추출합니다.
fn extract_dsaction_urls(content: &str) -> (HashMap<String, String>, Option<String>) {
    let mut map = HashMap::new();
    let mut reader = Reader::from_str(content);
    reader.config_mut().trim_text(true);

    let mut in_dsaction = false;
    let mut current_col_id: Option<String> = None;
    let mut current_row_id: Option<String> = None;
    let mut current_row_url: Option<String> = None;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let name = std::str::from_utf8(e.name().as_ref()).unwrap_or("").to_string();
                match name.as_str() {
                    "Dataset" => {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"id" {
                                let val = std::str::from_utf8(&attr.value).unwrap_or("");
                                if val == "dsAction" {
                                    in_dsaction = true;
                                }
                            }
                        }
                    }
                    "Row" if in_dsaction => {
                        current_row_id = None;
                        current_row_url = None;
                    }
                    "Col" if in_dsaction => {
                        current_col_id = None;
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"id" {
                                current_col_id =
                                    Some(std::str::from_utf8(&attr.value).unwrap_or("").to_string());
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(ref e)) if in_dsaction => {
                if let Some(ref col_id) = current_col_id.clone() {
                    let text = e.unescape().unwrap_or_default().to_string();
                    match col_id.as_str() {
                        "ID" => current_row_id = Some(text),
                        "URL" => current_row_url = Some(text),
                        _ => {}
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let name = std::str::from_utf8(e.name().as_ref()).unwrap_or("").to_string();
                match name.as_str() {
                    "Col" => current_col_id = None,
                    "Row" if in_dsaction => {
                        if let (Some(id), Some(url)) =
                            (current_row_id.take(), current_row_url.take())
                        {
                            map.insert(id, url);
                        }
                    }
                    "Dataset" => {
                        if in_dsaction {
                            in_dsaction = false;
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                let pos = reader.buffer_position() as usize;
                // \n 개수를 세어 대략적인 라인 번호 추정
                let line = content[..pos].chars().filter(|&c| c == '\n').count() + 1;
                let msg = format!("XML 파싱 오류 (라인 {} 부근, 위치 {}): {e}", line, pos);
                return (map, Some(msg));
            }
            _ => {}
        }
        buf.clear();
    }

    (map, None)
}

/// Script CDATA 내에서 actionSubmit / actionSubmitFR 호출을 추출합니다.
/// 반환: Vec<(raw_action_id, is_literal)>
fn extract_action_calls(content: &str) -> Result<Vec<(String, bool)>, String> {
    let mut results: Vec<(String, bool)> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for cdata_cap in CDATA_RE.captures_iter(content) {
        let script = &cdata_cap[1];
        for cap in ACTION_CALL_RE.captures_iter(script) {
            let (action_id, is_literal) = if let Some(m) = cap.get(2).or_else(|| cap.get(3)) {
                (m.as_str().to_string(), true)
            } else if let Some(m) = cap.get(4) {
                (m.as_str().to_string(), false)
            } else {
                continue;
            };

            if seen.insert(action_id.clone()) {
                results.push((action_id, is_literal));
            }
        }
    }

    Ok(results)
}
