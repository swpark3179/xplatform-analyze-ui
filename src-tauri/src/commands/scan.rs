use tauri::{AppHandle, Emitter};
use walkdir::WalkDir;

use crate::models::{ScanProgress, XfdlFile};

/// src/webapp/ui 하위의 모든 .xfdl 파일을 재귀 탐색합니다.
/// 진행률은 "scan_progress" Tauri 이벤트로 전달됩니다.
#[tauri::command]
pub async fn scan_xfdl_files(
    app: AppHandle,
    root_path: String,
) -> Result<Vec<XfdlFile>, String> {
    let ui_path = std::path::Path::new(&root_path)
        .join("src")
        .join("webapp")
        .join("ui");

    if !ui_path.exists() {
        return Err(format!(
            "UI 폴더를 찾을 수 없습니다: {}",
            ui_path.display()
        ));
    }

    // 먼저 전체 .xfdl 파일 수를 세어 total 계산
    let all_files: Vec<_> = WalkDir::new(&ui_path)
        .into_iter()
        .flatten()
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.to_lowercase() == "xfdl")
                .unwrap_or(false)
        })
        .collect();

    let total = all_files.len();
    let mut result = Vec::with_capacity(total);

    for (idx, entry) in all_files.into_iter().enumerate() {
        let path = entry.path().to_string_lossy().to_string();
        let name = entry
            .path()
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();

        // 진행 이벤트 emit
        let _ = app.emit(
            "scan_progress",
            ScanProgress {
                current: idx + 1,
                total,
                current_file: name.clone(),
            },
        );

        result.push(XfdlFile { path, name });
    }

    Ok(result)
}
