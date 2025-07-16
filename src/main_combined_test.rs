// 統合機能テスト - Bus Error原因特定用
use anyhow::Result;
use std::sync::{Arc, Mutex, mpsc, atomic::{AtomicBool, Ordering}};
use std::thread;
use std::time::Duration;

fn main() -> Result<()> {
    println!("=== 段階4: 統合機能テスト ===");
    
    // 段階的にテスト
    test_simple_monitoring()?;
    
    Ok(())
}

fn test_simple_monitoring() -> Result<()> {
    println!("1. 簡単な監視機能テスト開始...");
    
    // テキスト変化イベント
    #[derive(Debug, Clone)]
    enum TextChangeEvent {
        NewText(String),
        Error(String),
    }
    
    // 簡単なOCRエンジン
    struct SimpleOcrEngine {
        tesseract: Mutex<tesseract::Tesseract>,
    }
    
    impl SimpleOcrEngine {
        fn new() -> Result<Self> {
            let tesseract = tesseract::Tesseract::new(None, Some("eng"))?;
            Ok(Self {
                tesseract: Mutex::new(tesseract),
            })
        }
        
        fn recognize_text(&self, image: &image::DynamicImage) -> Result<String> {
            use std::env;
            
            // 一時ファイルに保存
            let temp_dir = env::temp_dir();
            let temp_path = temp_dir.join(format!("combined_test_{}.png", std::process::id()));
            image.save(&temp_path)?;
            
            // OCR実行（新しいインスタンス作成）
            let mut new_tesseract = tesseract::Tesseract::new(None, Some("eng"))?
                .set_image(temp_path.to_str().unwrap())?;
            
            let text = new_tesseract.get_text()?;
            
            // クリーンアップ
            let _ = std::fs::remove_file(&temp_path);
            
            Ok(text.lines()
                .map(|line| line.trim())
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>()
                .join("\n"))
        }
    }
    
    // 簡単なキャプチャ
    struct SimpleCapture;
    
    impl SimpleCapture {
        fn capture_area(&self, x: i32, y: i32, width: u32, height: u32) -> Result<image::DynamicImage> {
            use screenshots::Screen;
            
            let screens = Screen::all()?;
            let screen = screens.first().ok_or_else(|| anyhow::anyhow!("スクリーンなし"))?;
            let image = screen.capture_area(x, y, width, height)?;
            
            Ok(image::DynamicImage::ImageRgba8(image))
        }
    }
    
    // 監視アプリケーション
    struct MonitorApp {
        event_receiver: Option<mpsc::Receiver<TextChangeEvent>>,
        stop_signal: Arc<AtomicBool>,
        monitor_handle: Option<thread::JoinHandle<()>>,
        last_events: Vec<String>,
    }
    
    impl Default for MonitorApp {
        fn default() -> Self {
            Self {
                event_receiver: None,
                stop_signal: Arc::new(AtomicBool::new(false)),
                monitor_handle: None,
                last_events: Vec::new(),
            }
        }
    }
    
    impl eframe::App for MonitorApp {
        fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            // イベント処理
            if let Some(receiver) = &mut self.event_receiver {
                while let Ok(event) = receiver.try_recv() {
                    match event {
                        TextChangeEvent::NewText(text) => {
                            self.last_events.push(format!("[新規] {}", text));
                        }
                        TextChangeEvent::Error(msg) => {
                            self.last_events.push(format!("[エラー] {}", msg));
                        }
                    }
                    // 最新10件のみ保持
                    if self.last_events.len() > 10 {
                        self.last_events.remove(0);
                    }
                }
            }
            
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.heading("統合監視テスト");
                
                if ui.button("監視開始").clicked() && self.event_receiver.is_none() {
                    self.start_monitoring();
                }
                
                if ui.button("監視停止").clicked() {
                    self.stop_monitoring();
                }
                
                if ui.button("終了").clicked() {
                    self.stop_monitoring();
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                
                ui.separator();
                ui.label("最新イベント:");
                
                egui::ScrollArea::vertical()
                    .max_height(200.0)
                    .show(ui, |ui| {
                        for event in self.last_events.iter().rev() {
                            ui.label(event);
                        }
                    });
            });
        }
    }
    
    impl MonitorApp {
        fn start_monitoring(&mut self) {
            println!("   - 監視開始処理...");
            
            // チャンネル作成
            let (tx, rx) = mpsc::channel();
            self.event_receiver = Some(rx);
            
            // 停止シグナルリセット
            self.stop_signal.store(false, Ordering::Relaxed);
            let stop_signal = self.stop_signal.clone();
            
            // 監視スレッド開始
            let handle = thread::spawn(move || {
                println!("   - 監視スレッド開始");
                
                // OCRエンジン初期化
                let ocr_engine = match SimpleOcrEngine::new() {
                    Ok(engine) => engine,
                    Err(e) => {
                        let _ = tx.send(TextChangeEvent::Error(format!("OCR初期化エラー: {}", e)));
                        return;
                    }
                };
                
                let capture = SimpleCapture;
                let mut counter = 0;
                
                while !stop_signal.load(Ordering::Relaxed) {
                    counter += 1;
                    
                    // 1秒間隔
                    thread::sleep(Duration::from_millis(1000));
                    
                    if stop_signal.load(Ordering::Relaxed) {
                        break;
                    }
                    
                    // 画面キャプチャ
                    let image = match capture.capture_area(100, 100, 300, 100) {
                        Ok(img) => img,
                        Err(e) => {
                            let _ = tx.send(TextChangeEvent::Error(format!("キャプチャエラー: {}", e)));
                            continue;
                        }
                    };
                    
                    // OCR処理
                    match ocr_engine.recognize_text(&image) {
                        Ok(text) => {
                            if !text.is_empty() {
                                let _ = tx.send(TextChangeEvent::NewText(format!("{}: {}", counter, text)));
                            } else {
                                let _ = tx.send(TextChangeEvent::NewText(format!("{}: (空)", counter)));
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(TextChangeEvent::Error(format!("OCRエラー: {}", e)));
                        }
                    }
                }
                
                println!("   - 監視スレッド終了");
            });
            
            self.monitor_handle = Some(handle);
            println!("   - 監視開始完了");
        }
        
        fn stop_monitoring(&mut self) {
            println!("   - 監視停止処理...");
            
            self.stop_signal.store(true, Ordering::Relaxed);
            
            if let Some(handle) = self.monitor_handle.take() {
                let _ = handle.join();
            }
            
            self.event_receiver = None;
            println!("   - 監視停止完了");
        }
    }
    
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([500.0, 400.0])
            .with_title("統合監視テスト"),
        ..Default::default()
    };
    
    println!("   - 統合監視アプリ開始...");
    
    eframe::run_native(
        "Combined Test",
        options,
        Box::new(|_cc| {
            Ok(Box::new(MonitorApp::default()))
        }),
    ).map_err(|e| anyhow::anyhow!("統合テストエラー: {}", e))?;
    
    println!("1. 簡単な監視機能テスト完了");
    
    Ok(())
}