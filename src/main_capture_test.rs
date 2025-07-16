// 画面キャプチャテスト - Bus Error原因特定用
use anyhow::Result;

fn main() -> Result<()> {
    println!("=== 段階2: 画面キャプチャテスト ===");
    
    // 基本的な画面キャプチャテスト
    test_screen_capture()?;
    
    // GUI + 画面キャプチャテスト
    test_gui_with_capture()?;
    
    Ok(())
}

fn test_screen_capture() -> Result<()> {
    use screenshots::Screen;
    
    println!("1. 基本画面キャプチャテスト開始...");
    
    let screens = Screen::all()?;
    println!("   - {}個のスクリーンを検出", screens.len());
    
    if let Some(screen) = screens.first() {
        println!("   - プライマリスクリーン: {}x{}", 
                screen.display_info.width, screen.display_info.height);
        
        // 小さな領域をキャプチャ
        println!("   - 小領域キャプチャ実行中...");
        let _image = screen.capture_area(0, 0, 100, 100)?;
        println!("   - 小領域キャプチャ成功");
        
        // 全画面キャプチャ
        println!("   - 全画面キャプチャ実行中...");
        let _full_image = screen.capture()?;
        println!("   - 全画面キャプチャ成功");
    }
    
    println!("1. 画面キャプチャテスト完了");
    Ok(())
}

fn test_gui_with_capture() -> Result<()> {
    println!("2. GUI + 画面キャプチャ統合テスト開始...");
    
    struct CaptureTestApp {
        last_capture_info: String,
    }
    
    impl Default for CaptureTestApp {
        fn default() -> Self {
            Self {
                last_capture_info: "未実行".to_string(),
            }
        }
    }
    
    impl eframe::App for CaptureTestApp {
        fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.heading("画面キャプチャテスト");
                ui.label(format!("最後のキャプチャ: {}", self.last_capture_info));
                
                if ui.button("画面キャプチャ実行").clicked() {
                    match self.perform_capture() {
                        Ok(info) => self.last_capture_info = info,
                        Err(e) => self.last_capture_info = format!("エラー: {}", e),
                    }
                }
                
                if ui.button("終了").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        }
    }
    
    impl CaptureTestApp {
        fn perform_capture(&self) -> Result<String> {
            use screenshots::Screen;
            
            let screens = Screen::all()?;
            let screen = screens.first().ok_or_else(|| anyhow::anyhow!("スクリーンなし"))?;
            
            let _image = screen.capture_area(0, 0, 200, 200)?;
            
            Ok(format!("{}x{} 領域キャプチャ成功", 200, 200))
        }
    }
    
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([400.0, 300.0])
            .with_title("画面キャプチャテスト"),
        ..Default::default()
    };
    
    println!("   - GUI + キャプチャアプリ開始...");
    
    eframe::run_native(
        "Capture Test",
        options,
        Box::new(|_cc| {
            Ok(Box::new(CaptureTestApp::default()))
        }),
    ).map_err(|e| anyhow::anyhow!("GUI + キャプチャエラー: {}", e))?;
    
    println!("2. GUI + 画面キャプチャテスト完了");
    
    Ok(())
}