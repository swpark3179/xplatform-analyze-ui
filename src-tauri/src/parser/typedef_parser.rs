use quick_xml::events::Event;
use quick_xml::reader::Reader;
use std::collections::HashMap;

/// default_typedef.xml 파일을 파싱하여 prefixid → url_path 맵을 반환합니다.
/// url 속성이 "./"로 시작하는 <Service> 태그만 수집합니다.
pub fn parse_typedef(typedef_path: &str) -> Result<HashMap<String, String>, String> {
    let content = std::fs::read_to_string(typedef_path)
        .map_err(|e| format!("default_typedef.xml 읽기 실패: {e}"))?;

    let mut reader = Reader::from_str(&content);
    reader.config_mut().trim_text(true);

    let mut map: HashMap<String, String> = HashMap::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let tag = std::str::from_utf8(e.name().as_ref()).unwrap_or("").to_string();
                if tag == "Service" {
                    let mut prefixid: Option<String> = None;
                    let mut url_val: Option<String> = None;

                    for attr in e.attributes().flatten() {
                        match attr.key.as_ref() {
                            b"prefixid" => {
                                prefixid =
                                    Some(std::str::from_utf8(&attr.value).unwrap_or("").to_string());
                            }
                            b"url" => {
                                let v = std::str::from_utf8(&attr.value).unwrap_or("").to_string();
                                if v.starts_with("./") {
                                    url_val = Some(v);
                                }
                            }
                            _ => {}
                        }
                    }

                    if let (Some(pid), Some(url)) = (prefixid, url_val) {
                        // "./cm/cmc/cmcs" → "cm/cmc/cmcs"
                        let cleaned = url.trim_start_matches("./").to_string();
                        map.insert(pid, cleaned);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("XML 파싱 오류: {e}")),
            _ => {}
        }
        buf.clear();
    }

    Ok(map)
}
