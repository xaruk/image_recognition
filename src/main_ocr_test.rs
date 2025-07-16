// OCRテスト - Bus Error原因特定用
use anyhow::Result;

fn main() -> Result<()> {
    println!("=== 段階3: OCRテスト ===");
    
    // Tesseract初期化テスト
    test_tesseract_init()?;
    
    // 基本OCRテスト
    test_basic_ocr()?;
    
    // GUI + OCRテスト
    test_gui_with_ocr()?;
    
    Ok(())
}

fn test_tesseract_init() -> Result<()> {
    use tesseract::Tesseract;
    
    println!("1. Tesseract初期化テスト開始...");
    
    // 英語モードでテスト
    println!("   - 英語モード初期化...");
    let tess_eng = Tesseract::new(None, Some("eng"))?;
    println!("   - 英語モード初期化成功");
    drop(tess_eng);
    
    // 日本語モード試行
    println!("   - 日本語モード初期化試行...");
    match Tesseract::new(None, Some("jpn")) {
        Ok(tess_jpn) => {
            println!("   - 日本語モード初期化成功");
            drop(tess_jpn);
        }
        Err(e) => {
            println!("   - 日本語モード初期化失敗: {}", e);
        }
    }
    
    // 複合モード試行
    println!("   - 複合モード(jpn+eng)初期化試行...");
    match Tesseract::new(None, Some("jpn+eng")) {
        Ok(tess_combined) => {
            println!("   - 複合モード初期化成功");
            drop(tess_combined);
        }
        Err(e) => {
            println!("   - 複合モード初期化失敗: {}", e);
        }
    }
    
    println!("1. Tesseract初期化テスト完了");
    Ok(())
}

fn test_basic_ocr() -> Result<()> {
    use tesseract::Tesseract;
    use screenshots::Screen;
    use std::env;
    
    println!("2. 基本OCRテスト開始...");
    
    // 画面キャプチャしてOCR
    println!("   - 画面キャプチャ中...");
    let screens = Screen::all()?;
    let screen = screens.first().ok_or_else(|| anyhow::anyhow!("スクリーンなし"))?;
    let image = screen.capture_area(0, 0, 200, 200)?;
    
    // 一時ファイルに保存
    let temp_dir = env::temp_dir();
    let temp_path = temp_dir.join(format!("ocr_test_{}.png", std::process::id()));
    
    println!("   - 一時ファイル保存: {:?}", temp_path);
    image.save(&temp_path)?;
    
    // OCR実行
    println!("   - OCR実行中...");
    let mut tesseract = Tesseract::new(None, Some("eng"))?
        .set_image(temp_path.to_str().unwrap())?;
    
    let text = tesseract.get_text()?;
    println!("   - OCR結果: '{}'", text.trim());
    
    // クリーンアップ
    let _ = std::fs::remove_file(&temp_path);
    
    println!("2. 基本OCRテスト完了");
    Ok(())
}

fn test_gui_with_ocr() -> Result<()> {
    println!("3. GUI + OCR統合テスト開始...");
    
    struct OcrTestApp {
        last_ocr_result: String,
    }
    
    impl Default for OcrTestApp {
        fn default() -> Self {
            Self {
                last_ocr_result: "未実行".to_string(),
            }
        }
    }
    
    impl eframe::App for OcrTestApp {
        fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.heading("OCRテスト");
                ui.label(format!("最後のOCR結果: {}", self.last_ocr_result));
                
                if ui.button("OCR実行").clicked() {
                    match self.perform_ocr() {
                        Ok(result) => self.last_ocr_result = result,
                        Err(e) => self.last_ocr_result = format!("エラー: {}", e),
                    }
                }
                
                if ui.button("終了").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        }
    }
    
    impl OcrTestApp {
        fn perform_ocr(&self) -> Result<String> {
            use tesseract::Tesseract;
            use screenshots::Screen;
            use std::env;
            
            // 画面キャプチャ
            let screens = Screen::all()?;
            let screen = screens.first().ok_or_else(|| anyhow::anyhow!("スクリーンなし"))?;
            let image = screen.capture_area(100, 100, 300, 100)?;
            
            // 一時ファイル保存
            let temp_dir = env::temp_dir();
            let temp_path = temp_dir.join(format!("gui_ocr_test_{}.png", std::process::id()));
            image.save(&temp_path)?;
            
            // OCR実行
            let mut tesseract = Tesseract::new(None, Some("eng"))?
                .set_image(temp_path.to_str().unwrap())?;
            
            let text = tesseract.get_text()?;
            
            // クリーンアップ
            let _ = std::fs::remove_file(&temp_path);
            
            Ok(if text.trim().is_empty() {
                "テキスト未検出".to_string()
            } else {
                format!("検出: '{}'", text.trim())
            })
        }
    }
    
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([400.0, 300.0])
            .with_title("OCRテスト"),
        ..Default::default()
    };
    
    println!("   - GUI + OCRアプリ開始...");
    
    eframe::run_native(
        "OCR Test",
        options,
        Box::new(|_cc| {
            Ok(Box::new(OcrTestApp::default()))
        }),
    ).map_err(|e| anyhow::anyhow!("GUI + OCRエラー: {}", e))?;
    
    println!("3. GUI + OCRテスト完了");
    
    Ok(())
}