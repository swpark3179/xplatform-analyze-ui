// ─────────────────────────────────────────────
// 결과 구조체
// ─────────────────────────────────────────────
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct MethodCall {
    pub class_name: String,
    pub method_name: String,
}

#[derive(Debug, Clone)]
pub struct FoundQuery {
    pub dao_class: String,
    pub call_method: String,
    /// 쿼리 ID (String 리터럴) 또는 "[변수 확인 필요]" / "[ClassName.FIELD]"
    pub query_id: String,
}

#[derive(Debug, Clone)]
pub struct ClassAnalysisResult {
    pub class_file: String,
    pub references: Vec<MethodCall>,
    pub queries: Vec<FoundQuery>,
    pub logs: Vec<String>,
}

use cafebabe::{parse_class, attributes::AttributeData, bytecode::Opcode, constant_pool::Loadable};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

// ─────────────────────────────────────────────
// [변경 3] DAO 클래스 판별
// ─────────────────────────────────────────────

/// FrameworkDao / ProxyDao 를 직접 참조하는 클래스인지 판별합니다.
/// class_name 은 '/' 구분자 또는 '.' 구분자 모두 허용.
fn is_framework_dao_class(class_name: &str) -> bool {
    let normalized = class_name.replace('/', ".");
    matches!(
        normalized.as_str(),
        "com.shi.framework.service.FrameworkDao"
            | "com.shi.framework.service.ProxyDao"
    )
}

/// ServiceCallback 클래스인지 판별합니다.
fn is_service_callback_class(class_name: &str) -> bool {
    let normalized = class_name.replace('/', ".");
    normalized == "com.shi.framework.service.ServiceCallback"
}

// ─────────────────────────────────────────────
// [변경 4] 간단 값 추적 (스택 시뮬레이터)
// ─────────────────────────────────────────────

/// 오피코드 스트림에서 가능한 한 단순하게 "다음 메서드 호출에 전달될 첫 번째 인자"를 추적합니다.
/// - String LDC → 리터럴 값
/// - getstatic  → "[ClassName.FIELD]"
/// - 그 외      → "[변수 확인 필요]"
#[derive(Default)]
struct SimpleValueTracker {
    /// 가장 최근에 스택에 올라온 String-계열 후보들 (순서 유지)
    pending: Vec<String>,
}

impl SimpleValueTracker {
    fn push_literal(&mut self, s: String) {
        self.pending.push(s);
    }

    fn push_field(&mut self, class_name: &str, field_name: &str) {
        let label = format!("[{}.{}]", class_name.replace('/', "."), field_name);
        self.pending.push(label);
    }

    fn push_unknown(&mut self) {
        self.pending.push("[변수 확인 필요]".to_string());
    }

    /// 메서드 호출 전 스택 최상위 String 후보를 꺼냅니다.
    fn pop_for_call(&mut self) -> Option<String> {
        self.pending.pop()
    }

    fn clear(&mut self) {
        self.pending.clear();
    }
}

// ─────────────────────────────────────────────
// [변경 1] 모든 관련 class 파일 수집
// ─────────────────────────────────────────────

/// `stem_lower`와 정확히 일치하거나 `stem_lower$...` 형태인 모든 .class 파일을 반환합니다.
fn find_all_class_files(search_dir: &Path, stem: &str) -> Vec<PathBuf> {
    let stem_lower = stem.to_lowercase().replace('\\', "/");
    let mut result = Vec::new();

    for entry in WalkDir::new(search_dir).into_iter().flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("class") {
            continue;
        }
        let rel = path
            .strip_prefix(search_dir)
            .map(|p| p.with_extension("").to_string_lossy().replace('\\', "/").to_lowercase())
            .unwrap_or_default();

        // stem과 정확히 일치하거나, stem 뒤에 '$' 가 붙은 익명/내부 클래스
        if rel == stem_lower || rel.starts_with(&format!("{}$", stem_lower)) {
            result.push(path.to_path_buf());
        }
    }

    // 안정적인 순서: 기본 class 파일 먼저, 그 다음 $1, $2, ...
    result.sort();
    result
}

// ─────────────────────────────────────────────
// 핵심 분석 함수
// ─────────────────────────────────────────────

/// Java 파일에 대응하는 .class 파일들(익명 클래스 포함)을 탐색하고 파싱하여
/// 대상 메서드가 참조하는 모든 메서드 목록과 쿼리를 반환합니다.
///
/// * `extra_methods`: 해당 class 파일 내에서 추가로 분석할 메서드명 목록 (e.g. `["doit"]`)
pub fn analyze_class(
    java_file: &str,
    root_path: &str,
    method_name: &str,
) -> Result<ClassAnalysisResult, String> {
    analyze_class_with_extra(java_file, root_path, method_name, &[])
}

/// `extra_methods` 를 추가로 분석하는 확장 버전.
pub fn analyze_class_with_extra(
    java_file: &str,
    root_path: &str,
    method_name: &str,
    extra_methods: &[&str],
) -> Result<ClassAnalysisResult, String> {
    let mut logs: Vec<String> = Vec::new();

    let rel_path = extract_relative_path(java_file, "src/java/")
        .ok_or_else(|| format!("Java 파일 경로에서 'src/java/' 를 찾을 수 없음: {java_file}"))?;

    let stem = Path::new(&rel_path)
        .with_extension("")
        .to_string_lossy()
        .replace('\\', "/");

    let class_search_dir = Path::new(root_path).join("target").join("classes");

    // [변경 1] 익명 클래스까지 모두 수집
    let class_files = find_all_class_files(&class_search_dir, &stem);
    if class_files.is_empty() {
        return Err(format!(".class 파일을 찾을 수 없음: target/classes/{stem}*.class"));
    }

    logs.push(format!("[ANALYZER] Java  파일: {java_file}"));
    logs.push(format!("[ANALYZER] Class 파일 목록 ({} 개):", class_files.len()));
    for cf in &class_files {
        logs.push(format!("  - {}", cf.display()));
    }

    let mut references: Vec<MethodCall> = Vec::new();
    let mut queries: Vec<FoundQuery> = Vec::new();

    // 분석 대상 메서드명 집합
    let mut target_methods: Vec<&str> = vec![method_name];
    for &em in extra_methods {
        if !target_methods.contains(&em) {
            target_methods.push(em);
        }
    }

    // [변경 2] ServiceCallback 익명 클래스의 doit 메서드도 분석 대상에 포함
    // $N 파일이 존재하면 해당 파일에서 doit 을 찾아야 하므로 전체 class 파일 루프 내에서 처리
    let primary_class_file = class_files[0].display().to_string();

    for class_file_path in &class_files {
        let bytes = std::fs::read(class_file_path)
            .map_err(|e| format!(".class 파일 읽기 실패 ({}): {e}", class_file_path.display()))?;

        let parsed = parse_class(&bytes)
            .map_err(|e| format!(".class 파싱 실패 ({}): {e:?}", class_file_path.display()))?;

        // [변경 2] $N 파일은 doit 메서드를 자동 분석 (ServiceCallback 익명 클래스)
        let file_stem = class_file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let is_anon = file_stem.contains('$');

        for method in &parsed.methods {
            let mname = method.name.as_ref();

            // 분석 대상 메서드인지 확인
            // - 기본 class 파일: target_methods 에 있는 메서드
            // - $N 파일: target_methods 에 있는 메서드 + "doit" (ServiceCallback 익명 클래스)
            let should_analyze = target_methods.contains(&mname)
                || (is_anon && mname == "doit");

            if !should_analyze {
                continue;
            }

            logs.push(format!(
                "[ANALYZER] 메서드 분석: {} (in {})",
                mname,
                class_file_path.file_name().and_then(|n| n.to_str()).unwrap_or("?")
            ));

            for attr in &method.attributes {
                if let AttributeData::Code(code) = &attr.data {
                    let bytecode = code.bytecode.as_ref();
                    let Some(bc) = bytecode else { continue };

                    // [변경 4] 간단 값 추적기
                    let mut tracker = SimpleValueTracker::default();

                    for (_offset, opcode) in bc.opcodes.iter() {
                        // 값 추적: String LDC
                        match opcode {
                            Opcode::Ldc(Loadable::LiteralConstant(
                                cafebabe::constant_pool::LiteralConstant::String(s),
                            ))
                            | Opcode::LdcW(Loadable::LiteralConstant(
                                cafebabe::constant_pool::LiteralConstant::String(s),
                            )) => {
                                tracker.push_literal(s.to_string());
                            }
                            // [변경 4] getstatic: static final 상수 필드
                            Opcode::Getstatic(field_ref) => {
                                let descriptor = field_ref.name_and_type.descriptor.as_ref();
                                // String 타입 필드만 추적 (Ljava/lang/String;)
                                if descriptor == "Ljava/lang/String;" {
                                    tracker.push_field(
                                        field_ref.class_name.as_ref(),
                                        field_ref.name_and_type.name.as_ref(),
                                    );
                                } else {
                                    tracker.push_unknown();
                                }
                            }
                            // [변경 4] aload: 로컬 변수 로드 (String 여부 불명이므로 unknown)
                            Opcode::Aload(n) => {
                                // aload_0 은 보통 this, 나머지는 파라미터/로컬
                                if *n == 0 {
                                    // this 는 스택 추적에서 제외
                                } else {
                                    tracker.push_unknown();
                                }
                            }
                            _ => {}
                        }

                        // 메서드 호출 처리
                        let member_ref = match opcode {
                            Opcode::Invokevirtual(m) => Some(m),
                            Opcode::Invokestatic(m) => Some(m),
                            Opcode::Invokespecial(m) => Some(m),
                            Opcode::Invokeinterface(m, _) => Some(m),
                            _ => None,
                        };

                        if let Some(m) = member_ref {
                            let class_name = m.class_name.as_ref().replace('/', ".");
                            let method_n = m.name_and_type.name.as_ref();
                            let descriptor = m.name_and_type.descriptor.as_ref();

                            // [변경 3] DAO 판별: FrameworkDao / ProxyDao 기반
                            // 또는 기존 호환성 유지: class 파일 내 superclass 정보로도 보완
                            let dao_by_class = is_framework_dao_class(&class_name);
                            // 보조: 클래스명 suffix 방식도 병행 (하위 호환)
                            let dao_by_suffix = class_name.to_lowercase().ends_with("dao");

                            // FrameworkDao/ProxyDao 를 상속하는지 확인 (parsed.super_class 비교)
                            let super_is_dao = parsed
                                .super_class
                                .as_ref()
                                .map(|sc| is_framework_dao_class(sc.as_ref()))
                                .unwrap_or(false);

                            let is_dao_call = dao_by_class || super_is_dao || dao_by_suffix;

                            if is_dao_call {
                                // [변경 4] 첫 번째 파라미터가 String이든 아니든 쿼리 추출 시도
                                // descriptor 에서 첫 번째 파라미터 타입 확인
                                let first_param_is_string =
                                    descriptor.starts_with("(Ljava/lang/String;");

                                let query_id_opt = if first_param_is_string {
                                    // String 리터럴 또는 추적된 값 사용
                                    tracker.pop_for_call().or_else(|| Some("[변수 확인 필요]".to_string()))
                                } else {
                                    // 첫 번째 파라미터가 String이 아닌 경우
                                    // 스택 추적으로 얻은 값이 있으면 사용, 없으면 [변수 확인 필요]
                                    Some(
                                        tracker
                                            .pop_for_call()
                                            .unwrap_or_else(|| "[변수 확인 필요]".to_string()),
                                    )
                                };

                                if let Some(query_id) = query_id_opt {
                                    let ref_str = format!("{class_name}.{method_n}({query_id})");
                                    logs.push(format!("[ANALYZER]   → QUERY 발견: {ref_str}"));
                                    queries.push(FoundQuery {
                                        dao_class: class_name.clone(),
                                        call_method: method_n.to_string(),
                                        query_id,
                                    });
                                }
                            }

                            // [변경 2] ServiceCallback 생성자 감지 로그
                            if is_service_callback_class(&class_name) && method_n == "<init>" {
                                logs.push(format!(
                                    "[ANALYZER]   → ServiceCallback 생성 감지 (익명 클래스의 doit() 자동 분석됨)"
                                ));
                            }

                            let method_call = MethodCall {
                                class_name: class_name.clone(),
                                method_name: method_n.to_string(),
                            };
                            references.push(method_call);

                            tracker.clear();
                        }
                    }
                }
            }
        }
    }

    references.sort();
    references.dedup();

    logs.push(format!(
        "[ANALYZER] 메서드 '{}' 참조 총 {}건, 쿼리 {}건",
        method_name,
        references.len(),
        queries.len()
    ));

    Ok(ClassAnalysisResult {
        class_file: primary_class_file,
        references,
        queries,
        logs,
    })
}

/// 경로 문자열에서 marker 이후의 상대 경로를 추출합니다.
fn extract_relative_path(full_path: &str, marker: &str) -> Option<String> {
    let normalized = full_path.replace('\\', "/");
    let marker_norm = marker.replace('\\', "/");
    let pos = normalized.to_lowercase().find(&marker_norm.to_lowercase())?;
    Some(normalized[pos + marker_norm.len()..].to_string())
}
