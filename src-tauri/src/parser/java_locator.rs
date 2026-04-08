use std::collections::HashMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// URL에서 Java 서비스 파일 경로와 메서드명, 라인 번호를 추론합니다.
///
/// URL 형식: /{prefixid}/{fileId}/{methodName}  (예: /cmcs/0246/selectApprovalParam)
/// typedef_map: prefixid → url_path  (예: "cmcs" → "cm/cmc/cmcs")
/// root_path: 프로젝트 루트 경로
pub fn locate_java_service(
    url: &str,
    typedef_map: &HashMap<String, String>,
    root_path: &str,
) -> Result<JavaServiceLocation, String> {
    // URL 세그먼트 파싱
    let segments: Vec<&str> = url.trim_start_matches('/').split('/').collect();
    if segments.len() < 3 {
        return Err(format!("URL 세그먼트 부족 (최소 3개 필요): {url}"));
    }

    let prefix_id = segments[0];
    let file_id = segments[1];
    let method_name = segments[segments.len() - 1];

    // prefixid → 경로 조회
    let url_path = typedef_map
        .get(prefix_id)
        .ok_or_else(|| format!("prefixid '{prefix_id}' 를 default_typedef.xml에서 찾을 수 없음"))?;

    // Java 기본 경로: Root/src/java/com/shi/{url_path}/
    let base_java_dir = Path::new(root_path)
        .join("src")
        .join("java")
        .join("com")
        .join("shi")
        .join(url_path);

    // 대소문자 무시 탐색으로 실제 디렉토리 찾기
    let actual_dir = find_case_insensitive_dir(&base_java_dir)
        .ok_or_else(|| {
            eprintln!("[ERROR] Java 기반 경로를 찾을 수 없음: {}", base_java_dir.display());
            format!("Java 기반 경로를 찾을 수 없음: com/shi/{}", url_path)
        })?;

    // 파일명: {PREFIXID_대문자}{fileId}Service.java
    let expected_filename = format!("{}{}Service.java", prefix_id.to_uppercase(), file_id);

    // 대소문자 무시 파일 탐색
    let java_file = find_file_case_insensitive(&actual_dir, &expected_filename)
        .ok_or_else(|| {
            eprintln!("[ERROR] Java 서비스 파일을 찾을 수 없음: {} in {}", expected_filename, actual_dir.display());
            format!("Java 서비스 파일을 찾을 수 없음: {}", expected_filename)
        })?;

    // 메서드 라인 탐색
    let method_line = find_public_method_line(&java_file, method_name)?;

    Ok(JavaServiceLocation {
        java_file: java_file.to_string_lossy().to_string(),
        method_name: method_name.to_string(),
        method_line,
    })
}

/// URL 경로(쿼리·스킴 제거)가 `/system/` 으로 시작하는지 (대소문자 무시)
pub fn is_system_prefixed_service_url(url: &str) -> bool {
    extract_url_path_preserve_case(url)
        .map(|p| p.to_lowercase().starts_with("/system/"))
        .unwrap_or(false)
}

/// Diablo 등: `/system/foo/bar` → `com/shi/common/service` 하위 `FooService.java` 의 `bar` 메서드.
pub fn locate_system_common_service(url: &str, root_path: &str) -> Result<JavaServiceLocation, String> {
    let Some(path) = extract_url_path_preserve_case(url) else {
        return Err("URL에서 경로를 추출할 수 없음".to_string());
    };
    let path_lc = path.to_lowercase();
    if !path_lc.starts_with("/system/") {
        return Err("URL이 /system/ 으로 시작하지 않음".to_string());
    }
    let rest = path[path_lc.find("/system/").unwrap() + "/system/".len()..].trim_matches('/');
    let segments: Vec<&str> = rest.split('/').filter(|s| !s.is_empty()).collect();
    if segments.len() < 2 {
        return Err(format!(
            "/system/ 이후 세그먼트가 2개 미만입니다: {rest}"
        ));
    }
    let method_name = segments[segments.len() - 1].to_string();
    let service_seg = segments[0];
    let pascal = camel_segment_to_pascal_class_stem(service_seg);
    let expected_filename = if pascal.ends_with("Service") {
        format!("{}.java", pascal)
    } else {
        format!("{}Service.java", pascal)
    };

    let service_root = Path::new(root_path)
        .join("src")
        .join("java")
        .join("com")
        .join("shi")
        .join("common")
        .join("service");
    if !service_root.is_dir() {
        eprintln!("[ERROR] common/service 경로가 없습니다: {}", service_root.display());
        return Err("common/service 경로가 없습니다 (com.shi.common.service)".to_string());
    }

    let exp_lower = expected_filename.to_lowercase();
    let mut found: Option<PathBuf> = None;
    for e in WalkDir::new(&service_root).into_iter().filter_map(|e| e.ok()) {
        if !e.file_type().is_file() {
            continue;
        }
        if e.path().extension().and_then(|x| x.to_str()) != Some("java") {
            continue;
        }
        if e.file_name().to_string_lossy().to_lowercase() == exp_lower {
            found = Some(e.path().to_path_buf());
            break;
        }
    }

    let java_file = found.ok_or_else(|| {
        format!(
            "common/service 에서 파일을 찾을 수 없음: {}",
            expected_filename
        )
    })?;

    let method_line = find_public_or_protected_method_line(&java_file, &method_name)?;

    Ok(JavaServiceLocation {
        java_file: java_file.to_string_lossy().to_string(),
        method_name,
        method_line,
    })
}

fn extract_url_path_preserve_case(url: &str) -> Option<String> {
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
    Some(if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    })
}

/// `commonApproval` → `CommonApproval` (첫 글자 대문자, 이후 camel 유지)
fn camel_segment_to_pascal_class_stem(seg: &str) -> String {
    let mut out = String::with_capacity(seg.len());
    let mut chars = seg.chars();
    if let Some(c) = chars.next() {
        out.push(c.to_ascii_uppercase());
        out.extend(chars);
    }
    out
}

/// 경로의 각 컴포넌트를 대소문자 무시로 실제 디렉토리를 찾아 반환합니다.
fn find_case_insensitive_dir(target: &Path) -> Option<PathBuf> {
    let mut current = PathBuf::new();

    for comp in target.components() {
        if let std::path::Component::Normal(c) = comp {
            let comp_str = c.to_string_lossy().to_lowercase();
            // 현재 디렉토리에서 대소문자 무시 매칭
            match std::fs::read_dir(&current) {
                Ok(entries) => {
                    let found = entries
                        .flatten()
                        .find(|e| e.file_name().to_string_lossy().to_lowercase() == comp_str);
                    match found {
                        Some(e) => current = e.path(),
                        None => return None,
                    }
                }
                Err(_) => return None,
            }
        } else {
            // Prefix(C: 등), RootDir(\ 등), CurDir(.), ParentDir(..) 는 그대로 푸시
            current.push(comp);
        }
    }

    if current.is_dir() { Some(current) } else { None }
}

/// 디렉토리에서 대소문자를 무시하고 정확히 일치하는 파일을 찾습니다.
fn find_file_case_insensitive(dir: &Path, expected_filename: &str) -> Option<PathBuf> {
    let expected_lower = expected_filename.to_lowercase();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                if entry.file_name().to_string_lossy().to_lowercase() == expected_lower {
                    return Some(entry.path());
                }
            }
        }
    }
    None
}

/// Java 파일에서 public 메서드를 찾고 시작 라인 번호를 반환합니다.
fn find_public_method_line(java_file: &Path, method_name: &str) -> Result<u32, String> {
    find_public_or_protected_method_line(java_file, method_name)
}

/// `public` 또는 `protected` 메서드 시그니처에서 메서드명이 일치하는 줄(1-based)을 반환합니다.
pub fn find_public_or_protected_method_line(java_file: &Path, method_name: &str) -> Result<u32, String> {
    let content = std::fs::read_to_string(java_file)
        .map_err(|e| {
            let fname = java_file.file_name().and_then(|n| n.to_str()).unwrap_or("[알 수 없음]");
            eprintln!("[ERROR] Java 파일 읽기 실패 ({}): {e}", java_file.display());
            format!("Java 파일 읽기 실패 ({fname})")
        })?;

    let method_pattern = format!(" {}(", method_name);

    for (idx, line) in content.lines().enumerate() {
        let is_pub = line.contains("public ") && line.contains(&method_pattern);
        let is_prot = line.contains("protected ") && line.contains(&method_pattern);
        if is_pub || is_prot {
            return Ok((idx + 1) as u32);
        }
    }

    Err(format!("메서드 '{method_name}'를 파일에서 찾을 수 없음"))
}

// ─────────────────────────────────────────────
// 결과 구조체
// ─────────────────────────────────────────────
#[derive(Debug, Clone)]
pub struct JavaServiceLocation {
    pub java_file: String,
    pub method_name: String,
    pub method_line: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_prefixed_detection() {
        assert!(is_system_prefixed_service_url("/system/a/b"));
        assert!(is_system_prefixed_service_url("https://ex.com/SYSTEM/foo/bar"));
        assert!(!is_system_prefixed_service_url("/cmcs/0246/m"));
    }

    #[test]
    fn camel_segment_to_pascal_first_only() {
        assert_eq!(camel_segment_to_pascal_class_stem("commonApproval"), "CommonApproval");
    }
}
