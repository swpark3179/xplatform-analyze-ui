//! Spring MVC `@Controller` / `@RestController`의 `@RequestMapping` 계열에서
//! `.do` / `.dox` URL → 핸들러 메서드를 역추적하기 위한 인덱스.

use std::collections::HashMap;
use std::path::Path;
use regex::Regex;
use walkdir::WalkDir;

use super::java_locator::JavaServiceLocation;

/// Spring 매핑 어노테이션 이름 (FQCN `.RequestMapping(` 도 `\bRequestMapping\s*\(` 로 매칭)
const MAPPING_ANNOTATION_RE: &str =
    r"\b(GetMapping|PostMapping|PutMapping|DeleteMapping|PatchMapping|RequestMapping)\s*\(";

/// 정규화된 경로 키 → 동일 URL에 매핑된 핸들러 후보
#[derive(Debug, Clone)]
pub struct SpringMappingIndex {
    by_path: HashMap<String, Vec<JavaServiceLocation>>,
}

#[derive(Debug, Clone)]
pub enum SpringLookupError {
    NotFound,
    Ambiguous(Vec<JavaServiceLocation>),
}

/// URL이 `.do` / `.dox` 스타일(대소문자 무시)인지
pub fn is_spring_do_style_url(url: &str) -> bool {
    let path_only = extract_path_for_suffix_check(url);
    let lower = path_only.to_lowercase();
    lower.ends_with(".do") || lower.ends_with(".dox")
}

fn extract_path_for_suffix_check(url: &str) -> String {
    let t = url.trim();
    if let Some(rest) = t.strip_prefix("http://").or_else(|| t.strip_prefix("https://")) {
        if let Some(idx) = rest.find('/') {
            return rest[idx..].split('?').next().unwrap_or("").to_string();
        }
        return String::new();
    }
    t.split('?').next().unwrap_or("").to_string()
}

/// 조회용 키: 경로만, 소문자, 선행 `/` 하나, 쿼리 제거
pub fn normalize_url_key(url: &str) -> Option<String> {
    let mut path = url.trim().to_string();
    if path.is_empty() {
        return None;
    }
    if let Some(q) = path.find('?') {
        path.truncate(q);
    }
    if path.starts_with("http://") || path.starts_with("https://") {
        let after_scheme = if path.starts_with("https://") {
            &path[8..]
        } else {
            &path[7..]
        };
        let idx = after_scheme.find('/')?;
        path = after_scheme[idx..].to_string();
    }
    let path = path.trim();
    if path.is_empty() {
        return None;
    }
    let key = if path.starts_with('/') {
        path.to_lowercase()
    } else {
        format!("/{}", path.to_lowercase())
    };
    Some(key)
}

/// `root_path/src/java` 이하의 컨트롤러 매핑 인덱스 구축
pub fn build_spring_mapping_index(root_path: &str) -> Result<SpringMappingIndex, String> {
    let java_root = Path::new(root_path).join("src").join("java");
    if !java_root.is_dir() {
        return Err(format!(
            "Java 소스 경로가 없습니다: {}",
            java_root.display()
        ));
    }

    let mut by_path: HashMap<String, Vec<JavaServiceLocation>> = HashMap::new();

    for entry in WalkDir::new(&java_root)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let is_java = entry
            .path()
            .extension()
            .and_then(|e| e.to_str())
            == Some("java");
        if !is_java {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        if !file_has_controller(&content) {
            continue;
        }
        let java_path_str = entry.path().to_string_lossy().to_string();
        for (key, loc) in index_java_file(&java_path_str, &content) {
            by_path.entry(key).or_default().push(loc);
        }
    }

    Ok(SpringMappingIndex { by_path })
}

/// `src/java`가 없거나 인덱스 구축에 실패하면 빈 인덱스 (옵션 Diablo 루트 등).
pub fn try_build_spring_mapping_index(root_path: &str) -> SpringMappingIndex {
    let java_root = Path::new(root_path).join("src").join("java");
    if !java_root.is_dir() {
        return SpringMappingIndex::empty();
    }
    build_spring_mapping_index(root_path).unwrap_or_else(|_| SpringMappingIndex::empty())
}

impl SpringMappingIndex {
    pub fn empty() -> Self {
        Self {
            by_path: HashMap::new(),
        }
    }

    pub fn lookup(&self, service_url: &str) -> Result<JavaServiceLocation, SpringLookupError> {
        let Some(key) = normalize_url_key(service_url) else {
            return Err(SpringLookupError::NotFound);
        };

        if let Some(v) = self.by_path.get(&key) {
            return match v.len() {
                0 => Err(SpringLookupError::NotFound),
                1 => Ok(v[0].clone()),
                _ => Err(SpringLookupError::Ambiguous(v.clone())),
            };
        }

        // 선행 슬래시 없이 인덱싱된 희귀 케이스
        let alt = key.trim_start_matches('/').to_string();
        if alt != key {
            if let Some(v) = self.by_path.get(&alt) {
                return match v.len() {
                    0 => Err(SpringLookupError::NotFound),
                    1 => Ok(v[0].clone()),
                    _ => Err(SpringLookupError::Ambiguous(v.clone())),
                };
            }
        }

        Err(SpringLookupError::NotFound)
    }
}

fn file_has_controller(content: &str) -> bool {
    let re = Regex::new(r"@(?:[\w.]+\.)?(?:Controller|RestController)\b").unwrap();
    re.is_match(content)
}

fn index_java_file(java_file: &str, content: &str) -> Vec<(String, JavaServiceLocation)> {
    let lines: Vec<&str> = content.lines().collect();
    let Some(class_line) = find_controller_class_line(&lines) else {
        return vec![];
    };

    let class_ann_start = class_annotation_window_start(&lines, class_line);
    let class_block = lines[class_ann_start..=class_line].join("\n");
    let class_paths = extract_paths_from_annotation_block(&class_block);

    let Some(body_start_line) = find_class_body_start_line(&lines, class_line) else {
        return vec![];
    };

    let mut out = Vec::new();
    let mut depth: i32 = 1;
    let mut pending_ann: Vec<String> = Vec::new();

    for li in body_start_line..lines.len() {
        let line = lines[li];
        let trimmed = line.trim();

        if depth == 1 {
            if trimmed.starts_with('@') {
                pending_ann.push(line.to_string());
            } else if looks_like_method_line(trimmed) {
                if let Some(mname) = extract_method_name(line) {
                    let ann_text = pending_ann.join("\n");
                    if has_mapping_annotation(&ann_text) {
                        let method_paths = extract_paths_from_annotation_block(&ann_text);
                        let combined = combine_class_and_method_paths(&class_paths, &method_paths);
                        let method_line = (li + 1) as u32;
                        for p in combined {
                            let key = mapping_path_to_key(&p);
                            out.push((
                                key,
                                JavaServiceLocation {
                                    java_file: java_file.to_string(),
                                    method_name: mname.clone(),
                                    method_line,
                                },
                            ));
                        }
                    }
                }
                pending_ann.clear();
            } else if trimmed.contains(" class ") && trimmed.contains('{') {
                pending_ann.clear();
            } else if !trimmed.is_empty()
                && !trimmed.starts_with("//")
                && !trimmed.starts_with('*')
                && !trimmed.starts_with("/*")
                && trimmed.ends_with(';')
                && !looks_like_method_line(trimmed)
            {
                pending_ann.clear();
            }
        }

        depth += brace_delta_line(line);
        if depth < 1 {
            break;
        }
    }

    dedupe_locations_by_key(out)
}

fn dedupe_locations_by_key(mut v: Vec<(String, JavaServiceLocation)>) -> Vec<(String, JavaServiceLocation)> {
    v.sort_by(|a, b| {
        a.0.cmp(&b.0).then_with(|| {
            a.1
                .java_file
                .cmp(&b.1.java_file)
                .then_with(|| a.1.method_name.cmp(&b.1.method_name))
        })
    });
    v.dedup_by(|a, b| {
        a.0 == b.0 && a.1.java_file == b.1.java_file && a.1.method_name == b.1.method_name
    });
    v
}

fn class_annotation_window_start(lines: &[&str], class_line: usize) -> usize {
    let mut start = class_line.saturating_sub(40);
    while start < class_line && lines.get(start).map(|l| l.trim().starts_with("import ")).unwrap_or(false) {
        start += 1;
    }
    while start < class_line && lines.get(start).map(|l| l.trim().is_empty() || l.trim().starts_with("//")).unwrap_or(false) {
        start += 1;
    }
    let mut i = class_line;
    while i > 0 {
        i -= 1;
        let t = lines[i].trim();
        if t.is_empty() || t.starts_with("//") || t.starts_with('*') || t.starts_with("import ") || t.starts_with("package ") {
            continue;
        }
        if t.starts_with('@') {
            start = i;
            continue;
        }
        break;
    }
    start
}

fn find_controller_class_line(lines: &[&str]) -> Option<usize> {
    let class_re = Regex::new(
        r"^\s*(?:public|protected|private)?\s*(?:abstract\s+|static\s+|final\s+|sealed\s+|strictfp\s+)*class\s+(\w+)\b",
    )
    .ok()?;
    let ctrl_re =
        Regex::new(r"@(?:[\w.]+\.)?(?:Controller|RestController)\b").ok()?;

    let mut candidates: Vec<(usize, usize)> = Vec::new();
    for (idx, line) in lines.iter().enumerate() {
        if !class_re.is_match(line) {
            continue;
        }
        let win_lo = idx.saturating_sub(35);
        let window = lines[win_lo..=idx].join("\n");
        if ctrl_re.is_match(&window) {
            let lead = line.chars().take_while(|c| c.is_whitespace()).count();
            candidates.push((idx, lead));
        }
    }
    candidates
        .into_iter()
        .min_by_key(|(_, ws)| *ws)
        .map(|(i, _)| i)
}

fn find_class_body_start_line(lines: &[&str], class_line: usize) -> Option<usize> {
    let mut combined = String::new();
    for li in class_line..lines.len().min(class_line + 15) {
        combined.push_str(lines[li]);
        combined.push('\n');
        if let Some(pos) = combined.find('{') {
            let rel_line = combined[..=pos]
                .bytes()
                .filter(|&b| b == b'\n')
                .count();
            return Some(class_line + rel_line + 1);
        }
    }
    None
}

fn brace_delta_line(line: &str) -> i32 {
    let mut delta = 0i32;
    let mut in_string: Option<char> = None;
    let mut escape = false;
    let mut prev_slash = false;

    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];

        if let Some(q) = in_string {
            if escape {
                escape = false;
                i += 1;
                continue;
            }
            if c == '\\' {
                escape = true;
                i += 1;
                continue;
            }
            if c == q {
                in_string = None;
            }
            i += 1;
            continue;
        }

        if prev_slash {
            if c == '/' {
                return delta;
            }
            if c == '*' {
                i += 1;
                while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                    i += 1;
                }
                i = (i + 2).min(chars.len());
                prev_slash = false;
                continue;
            }
            prev_slash = false;
        }

        if c == '/' && i + 1 < chars.len() {
            if chars[i + 1] == '/' {
                return delta;
            }
            if chars[i + 1] == '*' {
                i += 2;
                while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                    i += 1;
                }
                i = (i + 2).min(chars.len());
                continue;
            }
        }

        if c == '"' || c == '\'' {
            in_string = Some(c);
            i += 1;
            continue;
        }

        match c {
            '{' => delta += 1,
            '}' => delta -= 1,
            _ => {}
        }
        i += 1;
    }
    delta
}

fn looks_like_method_line(trimmed: &str) -> bool {
    (trimmed.starts_with("public ") || trimmed.starts_with("protected "))
        && trimmed.contains('(')
        && !trimmed.contains(" class ")
}

fn extract_method_name(line: &str) -> Option<String> {
    let re = Regex::new(
        r"^\s*(?:public|protected)\s+(?:[\w\[\]<>?,\s.]+\s)+(\w+)\s*\(",
    )
    .ok()?;
    re.captures(line)
        .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
}

fn has_mapping_annotation(block: &str) -> bool {
    Regex::new(MAPPING_ANNOTATION_RE)
        .ok()
        .map(|re| re.is_match(block))
        .unwrap_or(false)
}

fn extract_paths_from_annotation_block(block: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let re = Regex::new(MAPPING_ANNOTATION_RE).unwrap();
    for cap in re.captures_iter(block) {
        let m = cap.get(0).unwrap();
        let name = cap.get(1).map(|g| g.as_str()).unwrap_or("RequestMapping");
        let inner_rest = &block[m.end()..];
        let Some(close_rel) = find_matching_paren_end(inner_rest) else {
            continue;
        };
        let inner = &inner_rest[..close_rel];
        let is_rm = name == "RequestMapping";
        paths.extend(extract_string_paths_from_args(inner, is_rm));
    }
    paths.sort();
    paths.dedup();
    paths
}

/// `s`는 첫 `(` 직후부터; 닫는 `)`의 인덱스(해당 `)` 제외)를 반환
fn find_matching_paren_end(s: &str) -> Option<usize> {
    let mut depth = 1i32;
    let mut in_string: Option<char> = None;
    let mut escape = false;
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if let Some(q) = in_string {
            if escape {
                escape = false;
                i += 1;
                continue;
            }
            if c == '\\' {
                escape = true;
                i += 1;
                continue;
            }
            if c == q {
                in_string = None;
            }
            i += 1;
            continue;
        }
        if c == '"' || c == '\'' {
            in_string = Some(c);
            i += 1;
            continue;
        }
        if c == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i = (i + 2).min(chars.len());
            continue;
        }
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn extract_string_paths_from_args(inner: &str, is_request_mapping: bool) -> Vec<String> {
    let mut out = Vec::new();
    if is_request_mapping {
        let re_vp = Regex::new(r#"(?:value|path)\s*=\s*"((?:[^"\\]|\\.)*)""#).unwrap();
        for cap in re_vp.captures_iter(inner) {
            if let Some(s) = cap.get(1) {
                let t = unescape_java_string(s.as_str());
                if is_spring_path_literal(&t) {
                    out.push(t);
                }
            }
        }
        for cap in Regex::new(r#"(?:value|path)\s*=\s*\{([^}]*)\}"#)
            .unwrap()
            .captures_iter(inner)
        {
            if let Some(m) = cap.get(1) {
                out.extend(strings_in_brace_list(m.as_str()));
            }
        }
    }
    let re_str = Regex::new(r#""((?:[^"\\]|\\.)*)""#).unwrap();
    for cap in re_str.captures_iter(inner) {
        if let Some(s) = cap.get(1) {
            let t = unescape_java_string(s.as_str());
            if is_spring_path_literal(&t) {
                out.push(t);
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

fn unescape_java_string(s: &str) -> String {
    let mut r = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\\' {
            if let Some(n) = it.next() {
                r.push(match n {
                    'n' => '\n',
                    't' => '\t',
                    'r' => '\r',
                    other => other,
                });
            }
        } else {
            r.push(c);
        }
    }
    r
}

fn strings_in_brace_list(s: &str) -> Vec<String> {
    let mut v = Vec::new();
    let re_str = Regex::new(r#""((?:[^"\\]|\\.)*)""#).unwrap();
    for cap in re_str.captures_iter(s) {
        if let Some(m) = cap.get(1) {
            let t = unescape_java_string(m.as_str());
            if is_spring_path_literal(&t) {
                v.push(t);
            }
        }
    }
    v
}

fn is_spring_path_literal(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let lower = s.to_lowercase();
    lower.ends_with(".do")
        || lower.ends_with(".dox")
        || s.starts_with('/')
        || s.starts_with("${")
}

fn combine_class_and_method_paths(class_paths: &[String], method_paths: &[String]) -> Vec<String> {
    let class_norms: Vec<String> = if class_paths.is_empty() {
        vec![String::new()]
    } else {
        class_paths.to_vec()
    };

    let mut out = Vec::new();
    for mp in method_paths {
        let m = mp.trim();
        if m.is_empty() {
            continue;
        }
        if m.starts_with('/') {
            out.push(m.to_string());
            for c in &class_norms {
                if c.is_empty() {
                    continue;
                }
                let joined = join_path_segments(c, m.trim_start_matches('/'));
                out.push(joined);
            }
        } else {
            for c in &class_norms {
                if c.is_empty() {
                    out.push(format!("/{}", m.trim_start_matches('/')));
                } else {
                    out.push(join_path_segments(c, m));
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

fn join_path_segments(class_prefix: &str, method_suffix: &str) -> String {
    let c = class_prefix.trim().trim_end_matches('/');
    let m = method_suffix.trim().trim_start_matches('/');
    if c.is_empty() {
        format!("/{m}")
    } else if m.is_empty() {
        format!("/{}", c.trim_start_matches('/'))
    } else {
        format!("/{}/{}", c.trim_start_matches('/'), m)
    }
}

fn mapping_path_to_key(p: &str) -> String {
    let mut s = p.trim().to_lowercase();
    if s.is_empty() {
        return s;
    }
    if !s.starts_with('/') {
        s = format!("/{}", s);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_query_and_host() {
        assert_eq!(
            normalize_url_key("https://x.com/App/foo.do?a=1").as_deref(),
            Some("/app/foo.do")
        );
        assert_eq!(normalize_url_key("/Bar.DOX").as_deref(), Some("/bar.dox"));
    }

    #[test]
    fn is_do_style() {
        // 기본 케이스
        assert!(is_spring_do_style_url("/test.do"));
        assert!(is_spring_do_style_url("/test.dox"));

        // 대소문자 무시
        assert!(is_spring_do_style_url("/test.DO"));
        assert!(is_spring_do_style_url("/test.Dox"));
        assert!(is_spring_do_style_url("/test.dOX"));

        // 쿼리 파라미터 포함
        assert!(is_spring_do_style_url("/test.do?id=123"));
        assert!(is_spring_do_style_url("/api/v1/save.dox?mode=all&debug=true"));

        // 프로토콜 포함 (Full URL)
        assert!(is_spring_do_style_url("http://localhost:8080/app/main.do"));
        assert!(is_spring_do_style_url("https://example.com/login.dox?redirect=/"));

        // 공백 처리
        assert!(is_spring_do_style_url("  /trimmed.do  "));

        // 실패 케이스 (매칭되지 않음)
        assert!(!is_spring_do_style_url("/test.jsp"));
        assert!(!is_spring_do_style_url("/test.do.txt")); // 끝이 아님
        assert!(!is_spring_do_style_url("/do/something")); // 확장자가 아님
        assert!(!is_spring_do_style_url("/"));
        assert!(!is_spring_do_style_url(""));
        assert!(!is_spring_do_style_url("https://example.com")); // 경로 없음
    }

    #[test]
    fn fqcn_restcontroller_requestmapping_indexes_dox_path() {
        let src = r#"package com.example;
@org.springframework.web.bind.annotation.RestController
public class EpLoginController {
    @org.springframework.web.bind.annotation.RequestMapping("/system/registePortalInfo.dox")
    public void registerPortal() {}
}
"#;
        let locs = index_java_file("EpLoginController.java", src);
        let keys: Vec<&str> = locs.iter().map(|(k, _)| k.as_str()).collect();
        assert!(
            keys.contains(&"/system/registeportalinfo.dox"),
            "expected key in {:?}",
            keys
        );
    }
}
