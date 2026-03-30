use tauri::{AppHandle, Emitter};
use std::collections::HashMap;
use std::path::Path;

use crate::models::{
    AnalysisResult, AnalysisStatus, AnalysisType, AnalyzeProgress, ExtractedAction, ExtractedCombo,
};
use crate::parser::{
    class_analyzer,
    java_locator,
    spring_controller_index::{self, SpringLookupError},
    typedef_parser,
    xfdl_parser,
};

/// Diablo 루트에서 Java가 해결된 경우 `service_url` 앞에 `Diablo/` 접두사를 붙입니다.
fn service_url_with_diablo_prefix(url: Option<String>) -> Option<String> {
    url.map(|u| {
        let u = u.trim();
        if u.starts_with("Diablo/") {
            return u.to_string();
        }
        let rest = u.trim_start_matches('/');
        format!("Diablo/{rest}")
    })
}

/// 선택된 XFDL 파일들을 분석하여 분석 결과를 반환합니다.
#[tauri::command]
pub async fn analyze_actions(
    app: AppHandle,
    root_path: String,
    xfdl_paths: Vec<String>,
    diablo_root_path: Option<String>,
) -> Result<Vec<AnalysisResult>, String> {
    // 분석 시작 직후 상태 표시
    let _ = app.emit(
        "analyze_progress",
        AnalyzeProgress {
            current: 0,
            total: 0,
            current_id: String::new(),
            status: "default_typedef.xml 파싱 중...".to_string(),
        },
    );

    let typedef_path = Path::new(&root_path)
        .join("src")
        .join("webapp")
        .join("ui")
        .join("default_typedef.xml");

    if !typedef_path.exists() {
        return Err(format!(
            "default_typedef.xml 파일이 존재하지 않습니다: {}",
            typedef_path.display()
        ));
    }

    let typedef_map =
        typedef_parser::parse_typedef(&typedef_path.to_string_lossy())
            .map_err(|e| format!("default_typedef.xml 파싱 실패: {e}"))?;

    let _ = app.emit(
        "analyze_progress",
        AnalyzeProgress {
            current: 0,
            total: 0,
            current_id: String::new(),
            status: "Spring @Controller 매핑 인덱싱 중...".to_string(),
        },
    );
    let spring_index = spring_controller_index::build_spring_mapping_index(&root_path)?;

    let diablo_root = diablo_root_path
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let spring_diablo = if let Some(dr) = &diablo_root {
        let _ = app.emit(
            "analyze_progress",
            AnalyzeProgress {
                current: 0,
                total: 0,
                current_id: String::new(),
                status: "Diablo Spring @Controller 매핑 인덱싱 중...".to_string(),
            },
        );
        spring_controller_index::try_build_spring_mapping_index(dr)
    } else {
        spring_controller_index::SpringMappingIndex::empty()
    };

    // 2) 각 XFDL 추출 → Action + Combo 목록 수집
    let mut all_actions: Vec<ExtractedAction> = Vec::new();
    let mut all_combos: Vec<ExtractedCombo> = Vec::new();
    let n_files = xfdl_paths.len();

    for (file_idx, xfdl_path) in xfdl_paths.iter().enumerate() {
        let _ = app.emit(
            "analyze_progress",
            AnalyzeProgress {
                current: file_idx,
                total: n_files,
                current_id: String::new(),
                status: format!("XFDL 파일 목록 추출 중 ({}/{})...", file_idx + 1, n_files),
            },
        );

        let xfdl_name = Path::new(xfdl_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();

        match xfdl_parser::parse_xfdl(xfdl_path, &xfdl_name) {
            Ok((actions, parse_err)) => {
                if parse_err.is_some() {
                    // 경고만 반영, 결과는 계속 사용
                }
                all_actions.extend(actions);
            }
            Err(e) => {
                all_actions.push(ExtractedAction {
                    result_id: format!("{xfdl_name}.ERROR"),
                    action_id: "ERROR".to_string(),
                    xfdl_path: xfdl_path.clone(),
                    xfdl_name: xfdl_name.clone(),
                    url: None,
                    is_manual: false,
                    xml_parse_err: Some(e.clone()),
                });
            }
        }

        // 콤보 호출 추출 (같은 파일 내용 다시 읽기)
        if let Ok(content) = std::fs::read_to_string(xfdl_path) {
            if let Ok(combos) = xfdl_parser::extract_combo_calls(&content, xfdl_path, &xfdl_name) {
                all_combos.extend(combos);
            }
        }
    }

    let total = all_actions.len() + all_combos.len();
    let mut results: Vec<AnalysisResult> = Vec::with_capacity(total);

    // 3) Action 분석
    for (idx, action) in all_actions.iter().enumerate() {
        let _ = app.emit(
            "analyze_progress",
            AnalyzeProgress {
                current: idx + 1,
                total,
                current_id: action.result_id.clone(),
                status: format!("분석 중 ({}/{})", idx + 1, total),
            },
        );
        let mut r = analyze_single_action(
            action,
            &typedef_map,
            &root_path,
            &spring_index,
            false,
        );
        if let Some(dr) = &diablo_root {
            if r.analysis_type == AnalysisType::ActionSubmit && r.java_file.is_none() {
                r = analyze_single_action(action, &typedef_map, dr, &spring_diablo, true);
                if r.java_file.is_some() {
                    r.service_url = service_url_with_diablo_prefix(r.service_url);
                }
            }
        }
        results.push(r);
    }

    // 4) Combo 분석 (Java 경로 분석 없음)
    for (idx, combo) in all_combos.iter().enumerate() {
        let _ = app.emit(
            "analyze_progress",
            AnalyzeProgress {
                current: all_actions.len() + idx + 1,
                total,
                current_id: combo.result_id.clone(),
                status: format!("분석 중 ({}/{})", all_actions.len() + idx + 1, total),
            },
        );
        results.push(analyze_single_combo(combo));
    }

    Ok(results)
}

/// 단일 콤보 호출에 대한 분석 (공통코드 또는 호출 쿼리만 표시, Java 분석 없음)
fn analyze_single_combo(combo: &ExtractedCombo) -> AnalysisResult {
    use crate::models::QueryUsage;

    if combo.is_common_code {
        AnalysisResult {
            result_id: combo.result_id.clone(),
            action_id: "combo".to_string(),
            xfdl_path: combo.xfdl_path.clone(),
            xfdl_name: combo.xfdl_name.clone(),
            service_url: None,
            status: AnalysisStatus::Found,
            java_file: None,
            class_file: None,
            method_name: None,
            method_line: None,
            queries: vec![],
            error_msg: None,
            debug_logs: vec![],
            analysis_type: AnalysisType::Combo,
            combo_param: Some(combo.param.clone()),
            is_common_code: Some(true),
        }
    } else {
        AnalysisResult {
            result_id: combo.result_id.clone(),
            action_id: "combo".to_string(),
            xfdl_path: combo.xfdl_path.clone(),
            xfdl_name: combo.xfdl_name.clone(),
            service_url: None,
            status: AnalysisStatus::Found,
            java_file: None,
            class_file: None,
            method_name: None,
            method_line: None,
            queries: vec![QueryUsage {
                call_path: vec![],
                query_id: combo.param.clone(),
            }],
            error_msg: None,
            debug_logs: vec![],
            analysis_type: AnalysisType::Combo,
            combo_param: None,
            is_common_code: Some(false),
        }
    }
}

/// 단일 Action에 대한 분석을 수행합니다.
/// `use_system_common_heuristic`: Diablo 루트일 때 true — `/system/` 비-.do URL은 `common/service` 휴리스틱 후 typedef 폴백.
fn analyze_single_action(
    action: &ExtractedAction,
    typedef_map: &HashMap<String, String>,
    root_path: &str,
    spring_index: &spring_controller_index::SpringMappingIndex,
    use_system_common_heuristic: bool,
) -> AnalysisResult {
    let mut debug_logs = Vec::new();

    if let Some(ref xml_err) = action.xml_parse_err {
        debug_logs.push(format!("⚠️ [경고] XFDL 부분 파싱 중 오류 발생 (이휴 과정 진행됨):\n{xml_err}"));
    }

    // 수동확인 대상
    if action.is_manual {
        return AnalysisResult {
            result_id: action.result_id.clone(),
            action_id: action.action_id.clone(),
            xfdl_path: action.xfdl_path.clone(),
            xfdl_name: action.xfdl_name.clone(),
            service_url: action.url.clone(),
            status: AnalysisStatus::ManualCheck,
            java_file: None,
            class_file: None,
            method_name: None,
            method_line: None,
            queries: vec![],
            error_msg: Some("actionSubmit 첫 번째 인자가 변수입니다. 수동 확인이 필요합니다.".to_string()),
            debug_logs,
            analysis_type: AnalysisType::ActionSubmit,
            combo_param: None,
            is_common_code: None,
        };
    }

    // URL 없으면 미발견
    let service_url = match &action.url {
        Some(u) => u.clone(),
        None => {
            return AnalysisResult {
                result_id: action.result_id.clone(),
                action_id: action.action_id.clone(),
                xfdl_path: action.xfdl_path.clone(),
                xfdl_name: action.xfdl_name.clone(),
                service_url: None,
                status: AnalysisStatus::NotFound,
                java_file: None,
                class_file: None,
                method_name: None,
                method_line: None,
                queries: vec![],
                error_msg: Some("dsAction Dataset에서 URL을 찾지 못했습니다.".to_string()),
                debug_logs,
                analysis_type: AnalysisType::ActionSubmit,
                combo_param: None,
                is_common_code: None,
            };
        }
    };

    debug_logs.push(format!("URL: {}", service_url));

    // Java 파일 위치 추론 (.do / .dox → Spring 매핑, 그 외 → typedef + *Service.java)
    let java_loc = if spring_controller_index::is_spring_do_style_url(&service_url) {
        match spring_index.lookup(&service_url) {
            Ok(loc) => loc,
            Err(SpringLookupError::NotFound) => {
                debug_logs.push(
                    "Spring: 문자열 리터럴 path/value로 매핑된 .do/.dox 핸들러를 찾지 못했습니다."
                        .to_string(),
                );
                return AnalysisResult {
                    result_id: action.result_id.clone(),
                    action_id: action.action_id.clone(),
                    xfdl_path: action.xfdl_path.clone(),
                    xfdl_name: action.xfdl_name.clone(),
                    service_url: Some(service_url),
                    status: AnalysisStatus::ManualCheck,
                    java_file: None,
                    class_file: None,
                    method_name: None,
                    method_line: None,
                    queries: vec![],
                    error_msg: Some(
                        "@Controller/@RestController의 @RequestMapping 등에서 해당 URL 문자열을 찾지 못했습니다."
                            .to_string(),
                    ),
                    debug_logs,
                    analysis_type: AnalysisType::ActionSubmit,
                    combo_param: None,
                    is_common_code: None,
                };
            }
            Err(SpringLookupError::Ambiguous(cands)) => {
                debug_logs.push(format!(
                    "Spring: 동일 URL에 복수 핸들러 후보 — {:?}",
                    cands
                ));
                return AnalysisResult {
                    result_id: action.result_id.clone(),
                    action_id: action.action_id.clone(),
                    xfdl_path: action.xfdl_path.clone(),
                    xfdl_name: action.xfdl_name.clone(),
                    service_url: Some(service_url),
                    status: AnalysisStatus::ManualCheck,
                    java_file: None,
                    class_file: None,
                    method_name: None,
                    method_line: None,
                    queries: vec![],
                    error_msg: Some(format!(
                        "동일 URL에 매핑된 핸들러가 여러 개입니다: {:?}",
                        cands
                    )),
                    debug_logs,
                    analysis_type: AnalysisType::ActionSubmit,
                    combo_param: None,
                    is_common_code: None,
                };
            }
        }
    } else {
        let try_system = use_system_common_heuristic
            && java_locator::is_system_prefixed_service_url(&service_url);
        let java_loc = if try_system {
            match java_locator::locate_system_common_service(&service_url, root_path) {
                Ok(loc) => {
                    debug_logs.push(format!("common/service 휴리스틱: {:?}", loc));
                    loc
                }
                Err(e_sys) => {
                    debug_logs.push(format!(
                        "common/service 휴리스틱 실패, typedef 경로 시도: {e_sys}"
                    ));
                    match java_locator::locate_java_service(&service_url, typedef_map, root_path) {
                        Ok(loc) => loc,
                        Err(e) => {
                            let status = if e.contains("default_typedef.xml에서 찾을 수 없음") {
                                AnalysisStatus::ManualCheck
                            } else {
                                AnalysisStatus::Error
                            };
                            return AnalysisResult {
                                result_id: action.result_id.clone(),
                                action_id: action.action_id.clone(),
                                xfdl_path: action.xfdl_path.clone(),
                                xfdl_name: action.xfdl_name.clone(),
                                service_url: Some(service_url),
                                status,
                                java_file: None,
                                class_file: None,
                                method_name: None,
                                method_line: None,
                                queries: vec![],
                                error_msg: Some(e),
                                debug_logs,
                                analysis_type: AnalysisType::ActionSubmit,
                                combo_param: None,
                                is_common_code: None,
                            };
                        }
                    }
                }
            }
        } else {
            match java_locator::locate_java_service(&service_url, typedef_map, root_path) {
                Ok(loc) => loc,
                Err(e) => {
                    let status = if e.contains("default_typedef.xml에서 찾을 수 없음") {
                        AnalysisStatus::ManualCheck
                    } else {
                        AnalysisStatus::Error
                    };
                    return AnalysisResult {
                        result_id: action.result_id.clone(),
                        action_id: action.action_id.clone(),
                        xfdl_path: action.xfdl_path.clone(),
                        xfdl_name: action.xfdl_name.clone(),
                        service_url: Some(service_url),
                        status,
                        java_file: None,
                        class_file: None,
                        method_name: None,
                        method_line: None,
                        queries: vec![],
                        error_msg: Some(e),
                        debug_logs,
                        analysis_type: AnalysisType::ActionSubmit,
                        combo_param: None,
                        is_common_code: None,
                    };
                }
            }
        };
        java_loc
    };

    debug_logs.push(format!("Java Locator: {:?}", java_loc));

    // .class 분석 (재귀적 탐색)
    let mut queries = Vec::new();
    let mut visited = std::collections::HashSet::new();
    let mut class_file = None;

    {
        // 첫 진입점
        let call_id = format!("{}#{}", java_loc.java_file, java_loc.method_name);
        visited.insert(call_id);

        // [변경 2] ServiceCallback doit() 을 추가 분석 대상으로 포함
        let res = class_analyzer::analyze_class_with_extra(
            &java_loc.java_file,
            root_path,
            &java_loc.method_name,
            &["doit"],
        );
        match res {
            Ok(r) => {
                debug_logs.extend(r.logs);
                class_file = Some(r.class_file);
                
                let class_disp = Path::new(&java_loc.java_file).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                let initial_path = vec![format!("{}.{}", class_disp, java_loc.method_name)];

                for q in r.queries {
                    let mut q_path = initial_path.clone();
                    let simple_dao = q.dao_class.split('.').last().unwrap_or(&q.dao_class);
                    q_path.push(format!("{}.{}", simple_dao, q.call_method));
                    queries.push(crate::models::QueryUsage {
                        call_path: q_path,
                        query_id: q.query_id,
                    });
                }

                for reference in r.references {
                    if !reference.class_name.starts_with("com.shi.") {
                        continue;
                    }
                    let rel_java_path = reference.class_name.replace('.', "/") + ".java";
                    let next_java_file = Path::new(root_path).join("src").join("java").join(rel_java_path);
                    if next_java_file.exists() {
                        analyze_recursive(
                            next_java_file.to_string_lossy().to_string(),
                            reference.method_name,
                            root_path,
                            initial_path.clone(),
                            &mut visited,
                            &mut queries,
                            &mut debug_logs,
                        );
                    }
                }
            }
            Err(e) => {
                debug_logs.push(format!("[ANALYZE] class 분석 실패: {e}"));
            }
        }
    }

    AnalysisResult {
        result_id: action.result_id.clone(),
        action_id: action.action_id.clone(),
        xfdl_path: action.xfdl_path.clone(),
        xfdl_name: action.xfdl_name.clone(),
        service_url: Some(service_url),
        status: AnalysisStatus::Found,
        java_file: Some(java_loc.java_file),
        class_file,
        method_name: Some(java_loc.method_name),
        method_line: Some(java_loc.method_line),
        queries,
        error_msg: None,
        debug_logs,
        analysis_type: AnalysisType::ActionSubmit,
        combo_param: None,
        is_common_code: None,
    }
}

/// 특정 메서드에서 호출하는 내부 메서드를 재귀적으로 탐색합니다.
fn analyze_recursive(
    java_file: String,
    method_name: String,
    root_path: &str,
    current_path: Vec<String>,
    visited: &mut std::collections::HashSet<String>,
    all_queries: &mut Vec<crate::models::QueryUsage>,
    debug_logs: &mut Vec<String>,
) {
    let call_id = format!("{}#{}", java_file, method_name);
    if visited.contains(&call_id) {
        return;
    }
    visited.insert(call_id.clone());

    // [변경 2] ServiceCallback doit() 을 추가 분석 대상으로 포함
    let (references, queries) = match class_analyzer::analyze_class_with_extra(
        &java_file,
        root_path,
        &method_name,
        &["doit"],
    ) {
        Ok(r) => {
            debug_logs.extend(r.logs);
            (r.references, r.queries)
        }
        Err(e) => {
            debug_logs.push(format!("[ANALYZE] 내부 class 분석 실패({}): {}", java_file, e));
            return;
        }
    };

    let class_disp = Path::new(&java_file).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let mut new_path = current_path.clone();
    new_path.push(format!("{}.{}", class_disp, method_name));

    for q in queries {
        let mut q_path = new_path.clone();
        let simple_dao = q.dao_class.split('.').last().unwrap_or(&q.dao_class);
        q_path.push(format!("{}.{}", simple_dao, q.call_method));
        all_queries.push(crate::models::QueryUsage {
            call_path: q_path,
            query_id: q.query_id,
        });
    }

    for reference in references {
        if !reference.class_name.starts_with("com.shi.") {
            continue;
        }

        let rel_java_path = reference.class_name.replace('.', "/") + ".java";
        let next_java_file = Path::new(root_path).join("src").join("java").join(rel_java_path);
        
        if next_java_file.exists() {
            analyze_recursive(
                next_java_file.to_string_lossy().to_string(),
                reference.method_name,
                root_path,
                new_path.clone(),
                visited,
                all_queries,
                debug_logs,
            );
        }
    }
}
