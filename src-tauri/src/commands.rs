//! Tauri command surface. Heavy work (reading/hashing the ~150 MB exe) runs on a
//! blocking thread so the UI never freezes. Note: `crate::patch` / `crate::detect`
//! are referenced by full path to avoid clashing with the same-named commands.

use crate::error::{AppError, AppResult};
use crate::model::*;
use std::path::Path;

#[tauri::command]
pub async fn detect() -> DetectResult {
    tauri::async_runtime::spawn_blocking(crate::detect::detect_all)
        .await
        .unwrap_or_else(|e| DetectResult {
            candidates: Vec::new(),
            steam_root: None,
            notes: vec![format!("Detection failed: {e}")],
        })
}

#[tauri::command]
pub async fn inspect_path(path: String) -> Result<GameInfo, AppError> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<GameInfo> {
        let backup_root = crate::patch::backup_root_dir();
        crate::patch::inspect(Path::new(&path), Store::Manual, &backup_root)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
pub fn compute(width: u32, height: u32, fov: Option<f64>) -> ComputedValues {
    crate::patch::compute_values(width, height, fov)
}

#[tauri::command]
pub async fn plan(path: String, options: PatchOptions) -> Result<PatchPlan, AppError> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<PatchPlan> {
        let bytes = std::fs::read(&path).map_err(AppError::from)?;
        Ok(crate::patch::build_plan(&bytes, &options))
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
pub async fn patch(path: String, options: PatchOptions) -> Result<PatchReport, AppError> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<PatchReport> {
        let backup_root = crate::patch::backup_root_dir();
        crate::patch::apply(Path::new(&path), &options, &backup_root)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
pub async fn revert(path: String) -> Result<PatchReport, AppError> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<PatchReport> {
        let backup_root = crate::patch::backup_root_dir();
        crate::patch::revert(Path::new(&path), &backup_root)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
pub fn is_running(path: String) -> bool {
    crate::patch::is_running(Path::new(&path))
}
