// Tauriベースの画面テキスト監視システム
#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use std::thread;
use std::time::Duration;
use tauri::{State, Window, Manager};
use log::info;

mod capture;
mod ocr;

use crate::capture::{CaptureRegion, ScreenCapture};
use crate::ocr::OcrEngine;

/// アプリケーションの状態
#[derive(Default)]
struct AppState {
    /// 選択中の領域
    selected_region: Option<CaptureRegion>,
    /// 監視が実行中かどうか
    is_monitoring: bool,
    /// 監視停止シグナル
    stop_monitoring: Arc<AtomicBool>,
    /// 監視スレッドのハンドル
    monitor_handle: Option<thread::JoinHandle<()>>,
}

/// テキスト変化イベント
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
enum TextChangeEvent {
    /// 新しいテキストが検出された
    #[serde(rename = "new")]
    NewText { text: String },
    /// テキストが変更された
    #[serde(rename = "changed")]
    TextChanged { old: String, new: String },
    /// テキストがクリアされた
    #[serde(rename = "cleared")]
    TextCleared { text: String },
    /// 情報メッセージ
    #[serde(rename = "info")]
    Info { message: String },
}

/// 領域選択のコマンド
#[tauri::command]
async fn select_region(state: State<'_, Mutex<AppState>>, app_handle: tauri::AppHandle) -> Result<CaptureRegion, String> {
    // 領域選択用のオーバーレイウィンドウを作成
    let region = match create_region_selector(app_handle).await {
        Ok(region) => region,
        Err(e) => return Err(format!("領域選択エラー: {}", e)),
    };
    
    let mut app_state = state.lock().map_err(|e| format!("状態ロックエラー: {}", e))?;
    app_state.selected_region = Some(region);
    
    info!("領域が選択されました: {:?}", region);
    Ok(region)
}

/// 領域選択用のオーバーレイウィンドウを作成
async fn create_region_selector(app_handle: tauri::AppHandle) -> Result<CaptureRegion> {
    use tauri::WindowBuilder;
    use tokio::sync::oneshot;
    
    // 結果を受け取るためのチャンネルを作成
    let (tx, rx) = oneshot::channel::<Option<CaptureRegion>>();
    let tx = Arc::new(Mutex::new(Some(tx)));
    
    // スクリーンのサイズを取得
    let screens = screenshots::Screen::all()
        .map_err(|e| anyhow::anyhow!("スクリーン情報の取得に失敗: {}", e))?;
    let primary_screen = screens.first()
        .ok_or_else(|| anyhow::anyhow!("プライマリスクリーンが見つかりません"))?;
    
    // オーバーレイウィンドウを作成（透明化とフルスクリーン）
    let overlay_window = WindowBuilder::new(
        &app_handle,
        "region_selector",
        tauri::WindowUrl::App("region_selector.html".into())
    )
    .title("領域を選択してください")
    .inner_size(primary_screen.display_info.width as f64, primary_screen.display_info.height as f64)
    .position(primary_screen.display_info.x as f64, primary_screen.display_info.y as f64)
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true) // タスクバーに表示しない
    .build()
    .map_err(|e| anyhow::anyhow!("オーバーレイウィンドウの作成に失敗: {}", e))?;
    
    // 透明化を試行
    #[cfg(target_os = "macos")]
    {
        use tauri::api::process::Command;
        // macOSでウィンドウを透明化
        if let Err(e) = Command::new("osascript")
            .args(["-e", &format!("tell application \"System Events\" to set background color of window \"{}\" to transparent", overlay_window.label())])
            .output() {
            log::warn!("透明化コマンドの実行に失敗: {}", e);
        }
    }
    
    // イベントリスナーを設定
    let tx_selected = tx.clone();
    let tx_cancelled = tx.clone();
    
    // 領域選択イベントのリスナー
    app_handle.listen_global("region-selected", move |event| {
        if let Some(payload) = event.payload() {
            info!("領域選択イベントを受信: payload={}", payload);
            
            // 複数の形式でパースを試行
            let region_result = serde_json::from_str::<CaptureRegion>(payload)
                .or_else(|_| {
                    // 二重エンコードされている場合の処理
                    info!("直接パースに失敗、二重エンコードを試行");
                    serde_json::from_str::<String>(payload)
                        .and_then(|s| {
                            info!("二重エンコードされた文字列: {}", s);
                            serde_json::from_str::<CaptureRegion>(&s)
                        })
                })
                .or_else(|_| {
                    // 型チェック付きでパース
                    info!("型チェック付きでパース試行");
                    serde_json::from_str::<serde_json::Value>(payload)
                        .and_then(|v| {
                            info!("パースした値: {:?}", v);
                            serde_json::from_value::<CaptureRegion>(v)
                        })
                });
            
            match region_result {
                Ok(region) => {
                    info!("領域のパースに成功: {:?}", region);
                    if let Ok(mut sender) = tx_selected.lock() {
                        if let Some(tx) = sender.take() {
                            let _ = tx.send(Some(region));
                        }
                    }
                }
                Err(e) => {
                    log::error!("すべてのパース方法に失敗: {}, payload={}", e, payload);
                }
            }
        }
    });
    
    // キャンセルイベントのリスナー
    app_handle.listen_global("region-cancelled", move |_| {
        if let Ok(mut sender) = tx_cancelled.lock() {
            if let Some(tx) = sender.take() {
                let _ = tx.send(None);
            }
        }
    });
    
    // 結果を待機（タイムアウト30秒）
    let result = tokio::time::timeout(Duration::from_secs(30), rx)
        .await
        .map_err(|_| anyhow::anyhow!("領域選択がタイムアウトしました"))?
        .map_err(|_| anyhow::anyhow!("領域選択の受信エラー"))?;
    
    // オーバーレイウィンドウを閉じる
    let _ = overlay_window.close();
    
    match result {
        Some(region) => {
            info!("領域選択が完了しました: {:?}", region);
            Ok(region)
        }
        None => Err(anyhow::anyhow!("領域選択がキャンセルされました"))
    }
}

/// 監視開始のコマンド
#[tauri::command]
fn start_monitoring(
    region: CaptureRegion,
    state: State<Mutex<AppState>>,
    window: Window,
) -> Result<(), String> {
    info!("監視開始コマンドが呼ばれました: region={:?}", region);
    
    let mut app_state = state.lock().map_err(|e| format!("状態ロックエラー: {}", e))?;
    
    if app_state.is_monitoring {
        return Err("既に監視が実行中です".to_string());
    }
    
    // 受け取った領域を保存
    app_state.selected_region = Some(region);
    
    // 停止シグナルをリセット
    app_state.stop_monitoring.store(false, Ordering::Relaxed);
    let stop_signal = app_state.stop_monitoring.clone();
    
    // 監視スレッドを起動
    let handle = thread::spawn(move || {
        info!("画面監視スレッドを開始しました: region={:?}", region);
        
        // OCRエンジンの初期化
        let ocr_engine = match OcrEngine::new() {
            Ok(engine) => engine,
            Err(e) => {
                let _ = window.emit("error", format!("OCR初期化エラー: {}", e));
                return;
            }
        };
        
        // 画面キャプチャの初期化（渡された領域を使用）
        let capture = ScreenCapture::new(region);
        let mut last_text: Option<String> = None;
        
        loop {
            // 停止シグナルをチェック
            if stop_signal.load(Ordering::Relaxed) {
                info!("監視停止シグナルを受信しました");
                break;
            }
            
            // 500ms間隔で監視
            thread::sleep(Duration::from_millis(500));
            
            // 再度停止シグナルをチェック
            if stop_signal.load(Ordering::Relaxed) {
                info!("監視停止シグナルを受信しました");
                break;
            }
            
            // 画面をキャプチャ
            let image = match capture.capture() {
                Ok(img) => img,
                Err(e) => {
                    log::error!("キャプチャエラー: {}", e);
                    let _ = window.emit("error", format!("キャプチャエラー: {}", e));
                    continue;
                }
            };
            
            // OCRでテキスト認識
            let current_text = match ocr_engine.recognize_text(&image) {
                Ok(text) => text,
                Err(e) => {
                    log::error!("OCRエラー: {}", e);
                    let _ = window.emit("error", format!("OCRエラー: {}", e));
                    continue;
                }
            };
            
            // 前回のテキストと比較
            match &last_text {
                None => {
                    // 初回認識
                    if !current_text.is_empty() {
                        info!("新しいテキストを検出: {}", current_text);
                        let event = TextChangeEvent::NewText { text: current_text.clone() };
                        let _ = window.emit("text-changed", event);
                        last_text = Some(current_text);
                    }
                }
                Some(prev_text) => {
                    if prev_text != &current_text {
                        if current_text.is_empty() {
                            // テキストがクリアされた
                            info!("テキストがクリアされました");
                            let event = TextChangeEvent::TextCleared { text: prev_text.clone() };
                            let _ = window.emit("text-changed", event);
                            last_text = None;
                        } else {
                            // テキストが変更された
                            info!("テキストが変更されました: {} -> {}", prev_text, current_text);
                            let event = TextChangeEvent::TextChanged {
                                old: prev_text.clone(),
                                new: current_text.clone(),
                            };
                            let _ = window.emit("text-changed", event);
                            last_text = Some(current_text);
                        }
                    }
                }
            }
        }
        
        info!("画面監視スレッドを終了しました");
    });
    
    app_state.monitor_handle = Some(handle);
    app_state.is_monitoring = true;
    
    Ok(())
}

/// 監視停止のコマンド
#[tauri::command]
fn stop_monitoring(state: State<Mutex<AppState>>) -> Result<(), String> {
    let mut app_state = state.lock().map_err(|e| format!("状態ロックエラー: {}", e))?;
    
    if !app_state.is_monitoring {
        return Err("監視が実行されていません".to_string());
    }
    
    // 停止シグナルを送信
    app_state.stop_monitoring.store(true, Ordering::Relaxed);
    app_state.is_monitoring = false;
    
    info!("監視停止を要求しました");
    Ok(())
}

fn main() {
    // ログの初期化
    env_logger::init();
    info!("Tauri版画面テキスト監視システムを起動しています...");
    
    tauri::Builder::default()
        .manage(Mutex::new(AppState::default()))
        .invoke_handler(tauri::generate_handler![
            select_region,
            start_monitoring,
            stop_monitoring
        ])
        .run(tauri::generate_context!())
        .expect("Tauriアプリケーションの起動エラー");
}