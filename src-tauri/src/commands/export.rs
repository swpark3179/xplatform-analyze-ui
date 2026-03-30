use rust_xlsxwriter::{Workbook, Format, Color};
use crate::models::{AnalysisResult, AnalysisStatus, AnalysisType};

/// 서비스 URL에서 prefix 추출: 맨 앞 `/` 제거 후 첫 번째 `/` 등장 전까지.
fn service_url_prefix(service_url: Option<&String>) -> String {
    let url = match service_url {
        Some(u) => u.as_str(),
        None => return String::new(),
    };
    let trimmed = url.trim_start_matches('/');
    trimmed.split('/').next().unwrap_or("").to_string()
}

/// 분석 결과를 Excel 파일로 저장합니다.
#[tauri::command]
pub async fn export_excel(
    results: Vec<AnalysisResult>,
    save_path: String,
) -> Result<(), String> {
    let mut workbook = Workbook::new();
    let worksheet = workbook.add_worksheet();
    worksheet.set_name("분석결과").map_err(|e| format!("시트 이름 설정 실패: {e}"))?;

    // 헤더 포맷
    let header_fmt = Format::new()
        .set_bold()
        .set_background_color(Color::RGB(0x5E81AC))
        .set_font_color(Color::White);

    let headers = [
        "분석ID", "ActionID", "유형", "XFDL 파일", "상태",
        "서비스 URL", "서비스 URL 접두사", "Java 파일", "메서드명", "라인번호",
        "쿼리 수", "호출 경로", "호출 쿼리", "오류 메시지",
    ];

    for (col, h) in headers.iter().enumerate() {
        worksheet
            .write_with_format(0, col as u16, *h, &header_fmt)
            .map_err(|e| format!("헤더 쓰기 실패: {e}"))?;
    }

    let mut current_row = 1;

    for r in results.iter() {
        let status_str = match r.status {
            AnalysisStatus::Found => "발견",
            AnalysisStatus::NotFound => "미발견",
            AnalysisStatus::ManualCheck => "수동확인",
            AnalysisStatus::Error => "오류",
        };

        let type_str = match r.analysis_type {
            AnalysisType::ActionSubmit => "actionSubmit",
            AnalysisType::Combo => "콤보",
        };

        let url_prefix = service_url_prefix(r.service_url.as_ref());

        // 콤보 + 공통코드 사용(예)인 경우 호출 쿼리란에 콤보 파라미터 표시
        let is_combo_common_code = matches!(r.analysis_type, AnalysisType::Combo)
            && r.is_common_code == Some(true);
        let call_query_for_combo_common = is_combo_common_code
            .then(|| r.combo_param.as_deref().unwrap_or(""))
            .unwrap_or("");

        if r.queries.is_empty() {
            // 쿼리가 없는 경우 1줄 출력 (콤보+공통코드면 호출 쿼리란에 콤보 파라미터)
            let cells: [&str; 14] = [
                &r.result_id,
                &r.action_id,
                type_str,
                &r.xfdl_name,
                status_str,
                r.service_url.as_deref().unwrap_or(""),
                &url_prefix,
                r.java_file.as_deref().unwrap_or(""),
                r.method_name.as_deref().unwrap_or(""),
                &r.method_line.map(|l| l.to_string()).unwrap_or_default(),
                "0",
                "",
                call_query_for_combo_common,
                r.error_msg.as_deref().unwrap_or(""),
            ];

            for (col, val) in cells.iter().enumerate() {
                worksheet
                    .write(current_row, col as u16, *val)
                    .map_err(|e| format!("셀 쓰기 실패: {e}"))?;
            }
            current_row += 1;
        } else {
            // 쿼리가 있는 경우 각 쿼리당 1줄 출력
            for q in &r.queries {
                let call_path_str = q.call_path.join(" -> ");
                let cells: [&str; 14] = [
                    &r.result_id,
                    &r.action_id,
                    type_str,
                    &r.xfdl_name,
                    status_str,
                    r.service_url.as_deref().unwrap_or(""),
                    &url_prefix,
                    r.java_file.as_deref().unwrap_or(""),
                    r.method_name.as_deref().unwrap_or(""),
                    &r.method_line.map(|l| l.to_string()).unwrap_or_default(),
                    &r.queries.len().to_string(),
                    &call_path_str,
                    &q.query_id,
                    r.error_msg.as_deref().unwrap_or(""),
                ];

                for (col, val) in cells.iter().enumerate() {
                    worksheet
                        .write(current_row, col as u16, *val)
                        .map_err(|e| format!("셀 쓰기 실패: {e}"))?;
                }
                current_row += 1;
            }
        }
    }

    // 컬럼 너비 자동 조정 (대략적인 값 설정)
    worksheet.set_column_width(0, 30).ok();
    worksheet.set_column_width(5, 50).ok();
    worksheet.set_column_width(9, 60).ok();

    workbook
        .save(&save_path)
        .map_err(|e| format!("Excel 저장 실패: {e}"))?;

    Ok(())
}
