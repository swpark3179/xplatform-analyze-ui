use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────
// XFDL 파일 정보
// ─────────────────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XfdlFile {
    pub path: String,
    pub name: String, // 확장자 제거한 파일명 (e.g. "CMCS0246")
}

// ─────────────────────────────────────────────
// actionSubmit 추출 결과
// ─────────────────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedAction {
    pub result_id: String,      // "CMCS0246.selectGrdMainList"
    pub action_id: String,      // "selectGrdMainList"
    pub xfdl_path: String,
    pub xfdl_name: String,      // 파일명(확장자 없음)
    pub url: Option<String>,    // dsAction Dataset에서 찾은 URL
    pub is_manual: bool,        // true = 첫 인자가 변수여서 수동확인 필요
    pub xml_parse_err: Option<String>, // XML 파싱 도중 에러가 나면 기록
}

// ─────────────────────────────────────────────
// getComCodeCombo / getGridCodeCombo 계열 추출 결과 (파라미터 1개당 1건)
// ─────────────────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedCombo {
    pub result_id: String,       // e.g. "CMCS0246.combo_0_CODE"
    pub xfdl_path: String,
    pub xfdl_name: String,
    pub param: String,          // 실제 파라미터 (| 앞부분, 9자 이하는 공통코드, 초과는 호출 쿼리)
    pub is_common_code: bool,    // true = 공통코드 사용, false = 호출 쿼리
}

// ─────────────────────────────────────────────
// 분석 상태
// ─────────────────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AnalysisStatus {
    Found,        // Java 파일 + 메서드 + class 모두 발견
    NotFound,     // Java 파일/메서드를 찾지 못함
    ManualCheck,  // actionSubmit 인자가 변수거나 prefixid 매핑 불가
    Error,        // 처리 중 오류
}

// ─────────────────────────────────────────────
// 쿼리 사용 위치 정보
// ─────────────────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryUsage {
    pub call_path: Vec<String>, // 호출 경로 ["A.method", "B.method", "Dao.method"]
    pub query_id: String,       // 호출 시 사용된 첫 번째 String 파라미터 (Query ID)
}

// ─────────────────────────────────────────────
// 분석 유형 (직렬화: "actionSubmit" | "combo")
// ─────────────────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AnalysisType {
    #[serde(rename = "actionSubmit")]
    ActionSubmit,
    #[serde(rename = "combo")]
    Combo,
}

// ─────────────────────────────────────────────
// 최종 분석 결과 (행 단위)
// ─────────────────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub result_id: String,
    pub action_id: String,
    pub xfdl_path: String,
    pub xfdl_name: String,
    pub service_url: Option<String>,
    pub status: AnalysisStatus,
    pub java_file: Option<String>,
    pub class_file: Option<String>,
    pub method_name: Option<String>,
    pub method_line: Option<u32>,
    pub queries: Vec<QueryUsage>, // 수집된 쿼리 목록
    pub error_msg: Option<String>,
    pub debug_logs: Vec<String>, // 화면 출력용 디버그 로그
    #[serde(default = "default_analysis_type")]
    pub analysis_type: AnalysisType,
    #[serde(default)]
    pub combo_param: Option<String>,  // 콤보 공통코드일 때 표시할 파라미터
    #[serde(default)]
    pub is_common_code: Option<bool>, // 콤보 건 중 공통코드 사용 여부 (필터용)
}

fn default_analysis_type() -> AnalysisType {
    AnalysisType::ActionSubmit
}

// ─────────────────────────────────────────────
// Tauri Event 페이로드
// ─────────────────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanProgress {
    pub current: usize,
    pub total: usize,
    pub current_file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzeProgress {
    pub current: usize,
    pub total: usize,
    pub current_id: String,
    pub status: String,
}
