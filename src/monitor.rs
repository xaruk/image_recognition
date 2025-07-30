// テキスト変化の監視機能の実装
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{interval, Duration};

use crate::capture::{ScreenCapture, CaptureRegion};
use crate::ocr::OcrEngine;

/// テキスト変化イベント
#[derive(Debug, Clone)]
pub enum TextChangeEvent {
    /// 新しいテキストが検出された
    NewText(String),
    /// テキストが変更された
    TextChanged { old: String, new: String },
    /// テキストがクリアされた
    TextCleared(String),
    /// 差分テキストが検出された（追加された部分のみ）
    DiffDetected { added: Vec<String>, removed: Vec<String> },
    /// エラーが発生した
    Error(String),
}

/// 画面監視を行う構造体
pub struct ScreenMonitor {
    /// 画面キャプチャ
    capture: ScreenCapture,
    /// OCRエンジン
    ocr_engine: Arc<OcrEngine>,
    /// 前回認識したテキスト
    last_text: Arc<RwLock<Option<String>>>,
    /// 監視間隔（ミリ秒）
    interval_ms: u64,
    /// テキスト差分検出器
    text_differ: TextDiffer,
}

impl ScreenMonitor {
    /// 新しいScreenMonitorを作成
    pub fn new(region: CaptureRegion, interval_ms: u64) -> Result<Self> {
        let capture = ScreenCapture::new(region);
        let ocr_engine = Arc::new(OcrEngine::new()?);
        let last_text = Arc::new(RwLock::new(None));
        let text_differ = TextDiffer::new(1); // 最小1文字の変更を検出

        Ok(Self {
            capture,
            ocr_engine,
            last_text,
            interval_ms,
            text_differ,
        })
    }

    /// 監視を開始
    pub async fn start_monitoring(
        &self,
        event_sender: mpsc::Sender<TextChangeEvent>,
    ) -> Result<()> {
        let mut interval = interval(Duration::from_millis(self.interval_ms));

        log::info!("画面監視を開始しました（間隔: {}ms）", self.interval_ms);

        loop {
            interval.tick().await;

            // 画面をキャプチャ
            let image = match self.capture.capture() {
                Ok(img) => img,
                Err(e) => {
                    log::error!("キャプチャエラー: {}", e);
                    let _ = event_sender.send(TextChangeEvent::Error(
                        format!("キャプチャエラー: {}", e)
                    )).await;
                    continue;
                }
            };

            // OCRでテキスト認識
            let current_text = match self.ocr_engine.recognize_text(&image) {
                Ok(text) => text,
                Err(e) => {
                    log::error!("OCRエラー: {}", e);
                    let _ = event_sender.send(TextChangeEvent::Error(
                        format!("OCRエラー: {}", e)
                    )).await;
                    continue;
                }
            };

            // 前回のテキストと比較
            let mut last_text = self.last_text.write().await;
            
            match &*last_text {
                None => {
                    // 初回認識
                    if !current_text.is_empty() {
                        log::info!("新しいテキストを検出: {}", current_text);
                        let _ = event_sender.send(TextChangeEvent::NewText(current_text.clone())).await;
                        *last_text = Some(current_text);
                    }
                }
                Some(prev_text) => {
                    if prev_text != &current_text {
                        if current_text.is_empty() {
                            // テキストがクリアされた
                            log::info!("テキストがクリアされました");
                            let _ = event_sender.send(TextChangeEvent::TextCleared(prev_text.clone())).await;
                            *last_text = None;
                        } else {
                            // テキストが変更された
                            log::info!("テキストが変更されました: {} -> {}", prev_text, current_text);
                            
                            // 差分を検出
                            let (added, removed) = self.text_differ.detect_changes(prev_text, &current_text);
                            
                            // 差分がある場合は差分イベントも送信
                            if !added.is_empty() || !removed.is_empty() {
                                log::info!("差分検出 - 追加: {:?}, 削除: {:?}", added, removed);
                                let _ = event_sender.send(TextChangeEvent::DiffDetected {
                                    added: added.clone(),
                                    removed: removed.clone(),
                                }).await;
                            }
                            
                            // 通常の変更イベントも送信
                            let _ = event_sender.send(TextChangeEvent::TextChanged {
                                old: prev_text.clone(),
                                new: current_text.clone(),
                            }).await;
                            
                            *last_text = Some(current_text);
                        }
                    }
                }
            }
        }
    }

    /// 監視領域を更新
    #[allow(dead_code)]
    pub fn update_region(&mut self, region: CaptureRegion) {
        self.capture = ScreenCapture::new(region);
        log::info!("監視領域を更新しました: {:?}", region);
    }

    /// 監視間隔を更新
    #[allow(dead_code)]
    pub fn update_interval(&mut self, interval_ms: u64) {
        self.interval_ms = interval_ms;
        log::info!("監視間隔を更新しました: {}ms", interval_ms);
    }
}

/// テキスト差分を検出するユーティリティ
#[allow(dead_code)]
pub struct TextDiffer {
    /// 最小変更文字数（これ以下の変更は無視）
    min_change_length: usize,
}

#[allow(dead_code)]
impl TextDiffer {
    /// 新しいTextDifferを作成
    pub fn new(min_change_length: usize) -> Self {
        Self { min_change_length }
    }

    /// 2つのテキストの差分を検出
    pub fn detect_changes(&self, old_text: &str, new_text: &str) -> (Vec<String>, Vec<String>) {
        let old_lines: Vec<&str> = old_text.lines().collect();
        let new_lines: Vec<&str> = new_text.lines().collect();

        let mut added = Vec::new();
        let mut removed = Vec::new();

        // 新しく追加された行を検出
        for new_line in &new_lines {
            if !old_lines.contains(new_line) && new_line.len() >= self.min_change_length {
                added.push(new_line.to_string());
            }
        }

        // 削除された行を検出
        for old_line in &old_lines {
            if !new_lines.contains(old_line) && old_line.len() >= self.min_change_length {
                removed.push(old_line.to_string());
            }
        }

        (added, removed)
    }
}