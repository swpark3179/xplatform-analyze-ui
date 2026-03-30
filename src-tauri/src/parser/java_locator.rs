use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
        .ok_or_else(|| format!("Java 기반 경로를 찾을 수 없음: {}", base_java_dir.display()))?;

    // 파일명: {PREFIXID_대문자}{fileId}Service.java
    let expected_filename = format!("{}{}Service.java", prefix_id.to_uppercase(), file_id);

    // 대소문자 무시 파일 탐색
    let java_file = find_file_case_insensitive(&actual_dir, &expected_filename)
        .ok_or_else(|| {
            format!(
                "Java 서비스 파일을 찾을 수 없음: {} in {}",
                expected_filename,
                actual_dir.display()
            )
        })?;

    // 메서드 라인 탐색
    let method_line = find_public_method_line(&java_file, method_name)?;

    Ok(JavaServiceLocation {
        java_file: java_file.to_string_lossy().to_string(),
        method_name: method_name.to_string(),
        method_line,
    })
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
        .map_err(|e| format!("Java 파일 읽기 실패: {e}"))?;

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
