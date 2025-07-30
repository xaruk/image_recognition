// OCR（光学文字認識）機能の実装
use anyhow::{Result, Context};
use image::{DynamicImage, ImageBuffer, Luma};
use tesseract::Tesseract;
use std::fs;
use std::env;

/// OCRエンジンのラッパー構造体
pub struct OcrEngine {
    // Tesseractは毎回新しいインスタンスを作成するため、フィールドは不要
    // （Bus Error回避のため、共有インスタンスではなく都度作成方式を採用）
}

impl OcrEngine {
    /// 新しいOCRエンジンを作成
    pub fn new() -> Result<Self> {
        // Tesseractの動作確認（初期化テスト）- 日本語のみ対応
        let _test_tesseract = Tesseract::new(None, Some("jpn"))
            .context("Tesseract（日本語）の初期化テストに失敗しました")?;
        
        log::info!("Tesseractの動作確認が完了しました（Bus Error回避）");

        Ok(Self {})
    }

    /// 画像から文字を認識
    pub fn recognize_text(&self, image: &DynamicImage) -> Result<String> {
        // 画像の前処理
        let processed_image = self.preprocess_image(image)?;

        // 複数回認識で精度向上
        self.recognize_with_multiple_attempts(&processed_image)
    }

    /// 複数回認識による精度向上
    fn recognize_with_multiple_attempts(&self, image: &DynamicImage) -> Result<String> {
        let mut results = Vec::new();
        
        // 3回認識を試行
        for i in 0..3 {
            match self.recognize_with_fallback(image) {
                Ok(text) => {
                    if !text.trim().is_empty() {
                        results.push(text);
                    }
                }
                Err(e) => {
                    log::warn!("認識試行 {} 失敗: {}", i + 1, e);
                }
            }
        }
        
        if results.is_empty() {
            return Err(anyhow::anyhow!("すべての認識試行が失敗しました"));
        }
        
        // 最も頻度の高い結果を選択
        if results.len() == 1 {
            Ok(results[0].clone())
        } else {
            // 複数の結果から最適なものを選択
            self.select_best_result(&results)
        }
    }

    /// 複数の認識結果から最適なものを選択
    fn select_best_result(&self, results: &[String]) -> Result<String> {
        use std::collections::HashMap;
        
        // 行ごとに最も頻度の高いものを選択
        let max_lines = results.iter().map(|r| r.lines().count()).max().unwrap_or(0);
        let mut best_lines = Vec::new();
        
        for line_idx in 0..max_lines {
            let mut line_counts = HashMap::new();
            
            for result in results {
                if let Some(line) = result.lines().nth(line_idx) {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        *line_counts.entry(trimmed.to_string()).or_insert(0) += 1;
                    }
                }
            }
            
            // 最も頻度の高い行を選択
            if let Some((best_line, _)) = line_counts.iter().max_by_key(|(_, count)| *count) {
                best_lines.push(best_line.clone());
            }
        }
        
        Ok(best_lines.join("\n"))
    }

    /// フォールバック方式でのOCR認識（複数の方法を試行）
    fn recognize_with_fallback(&self, image: &DynamicImage) -> Result<String> {
        // 方法1: BMPフォーマットでの保存を試行
        match self.try_bmp_recognition(image) {
            Ok(text) => {
                log::debug!("BMP方式での認識が成功しました");
                return Ok(text);
            }
            Err(e) => {
                log::warn!("BMP方式での認識に失敗: {}", e);
            }
        }

        // 方法2: より簡素な画像で再試行
        match self.try_simplified_recognition(image) {
            Ok(text) => {
                log::debug!("簡素化方式での認識が成功しました");
                return Ok(text);
            }
            Err(e) => {
                log::warn!("簡素化方式での認識に失敗: {}", e);
            }
        }

        // 全ての方法が失敗した場合
        Err(anyhow::anyhow!("全てのOCR方式が失敗しました"))
    }

    /// BMP方式でのOCR認識
    fn try_bmp_recognition(&self, image: &DynamicImage) -> Result<String> {
        let temp_dir = env::temp_dir();
        let temp_path = temp_dir.join(format!("ocr_temp_{}.bmp", std::process::id()));
        
        // より安全な画像保存（ImageIO EXC_BAD_ACCESS回避）
        image.save_with_format(&temp_path, image::ImageFormat::Bmp)
            .context("BMP画像の保存に失敗しました")?;

        let temp_path_str = temp_path.to_str()
            .context("一時ファイルパスの変換に失敗しました")?;
        
        // ファイルの存在確認
        if !temp_path.exists() {
            return Err(anyhow::anyhow!("一時ファイルが作成されませんでした"));
        }

        // Tesseractでの認識実行（日本語のみ対応）
        let mut tesseract = Tesseract::new(None, Some("jpn"))
            .context("Tesseract（日本語）の初期化に失敗しました")?;
        
        // OCRエンジンモード設定（より高精度なLSTM OCRエンジンを使用）
        tesseract = tesseract.set_variable("tessedit_ocr_engine_mode", "2")?; // 2 = Legacy + LSTM
        
        // ページセグメンテーションモード設定
        // 6 = 均一なブロックの単一テキスト（YouTubeチャット向け）
        tesseract = tesseract.set_variable("tessedit_pageseg_mode", "6")?;
        
        // 日本語認識の最適化設定
        tesseract = tesseract.set_variable("preserve_interword_spaces", "1")?; // 単語間スペースを保持
        tesseract = tesseract.set_variable("tessedit_char_whitelist", "")?; // 全文字を許可
        
        let mut tesseract_with_image = tesseract.set_image(temp_path_str)
            .context("画像の設定に失敗しました")?;
        
        let text = tesseract_with_image.get_text()
            .context("テキストの取得に失敗しました")?;

        // 一時ファイルを削除
        let _ = fs::remove_file(&temp_path);

        Ok(self.normalize_text(&text))
    }

    /// より簡素な方式でのOCR認識（最小限の処理）
    fn try_simplified_recognition(&self, image: &DynamicImage) -> Result<String> {
        // 画像を極めて小さくしてメモリ使用量を削減
        let small_image = image.resize(200, 100, image::imageops::FilterType::Nearest);
        
        let temp_dir = env::temp_dir();
        let temp_path = temp_dir.join(format!("ocr_simple_{}.bmp", std::process::id()));
        
        small_image.save_with_format(&temp_path, image::ImageFormat::Bmp)
            .context("簡素画像の保存に失敗しました")?;

        let temp_path_str = temp_path.to_str()
            .context("簡素ファイルパスの変換に失敗しました")?;

        let mut tesseract = Tesseract::new(None, Some("jpn"))
            .context("簡素Tesseract（日本語）の初期化に失敗しました")?;
        
        // 簡素版でも基本的な設定を適用
        tesseract = tesseract.set_variable("tessedit_ocr_engine_mode", "2")?;
        tesseract = tesseract.set_variable("tessedit_pageseg_mode", "6")?;
        
        let mut tesseract_with_image = tesseract.set_image(temp_path_str)
            .context("簡素画像の設定に失敗しました")?;
        
        let text = tesseract_with_image.get_text()
            .context("簡素テキストの取得に失敗しました")?;

        let _ = fs::remove_file(&temp_path);

        Ok(self.normalize_text(&text))
    }

    /// 画像の前処理（OCR精度向上のため）
    fn preprocess_image(&self, image: &DynamicImage) -> Result<DynamicImage> {
        use image::imageops;

        // 画像サイズの事前チェック（メモリ安全性）
        if image.width() == 0 || image.height() == 0 {
            return Err(anyhow::anyhow!("無効な画像サイズ: {}x{}", image.width(), image.height()));
        }

        // 画像が巨大すぎる場合は拒否（メモリ保護）
        if image.width() > 4096 || image.height() > 4096 {
            return Err(anyhow::anyhow!("画像サイズが大きすぎます: {}x{}", image.width(), image.height()));
        }

        let mut processed = image.clone();

        // 1. グレースケール変換
        processed = processed.grayscale();

        // 2. 解像度の最適化（OCR向けに高解像度化）
        if processed.width() < 1000 { // OCRは高解像度の方が精度が高い
            let scale_factor = 1000.0 / processed.width() as f32;
            let safe_scale_factor = scale_factor.min(4.0).max(1.5); // 1.5倍〜4倍に制限
            
            let new_width = (processed.width() as f32 * safe_scale_factor) as u32;
            let new_height = (processed.height() as f32 * safe_scale_factor) as u32;
            
            if new_width <= 3000 && new_height <= 3000 {
                processed = processed.resize(new_width, new_height, imageops::FilterType::Lanczos3);
            }
        }

        // 3. コントラスト強化と二値化
        let gray_image = processed.to_luma8();
        let enhanced = self.enhance_contrast(&gray_image)?;
        
        // 4. ノイズ除去（メディアンフィルタの簡易実装）
        let denoised = self.denoise_image(&enhanced)?;
        
        // 5. シャープネス強化
        let sharpened = self.sharpen_image(&denoised)?;

        log::debug!("画像前処理完了: {}x{}", sharpened.width(), sharpened.height());
        Ok(DynamicImage::ImageLuma8(sharpened))
    }

    /// コントラスト強化と適応的二値化
    fn enhance_contrast(&self, image: &ImageBuffer<Luma<u8>, Vec<u8>>) -> Result<ImageBuffer<Luma<u8>, Vec<u8>>> {
        let mut output = image.clone();
        
        // ヒストグラムを計算
        let mut histogram = vec![0u32; 256];
        for pixel in image.pixels() {
            histogram[pixel[0] as usize] += 1;
        }
        
        // 累積分布関数を計算
        let total_pixels = (image.width() * image.height()) as f32;
        let mut cdf = vec![0.0; 256];
        let mut sum = 0.0;
        for i in 0..256 {
            sum += histogram[i] as f32 / total_pixels;
            cdf[i] = sum;
        }
        
        // ヒストグラム均等化
        for pixel in output.pixels_mut() {
            let value = pixel[0] as usize;
            pixel[0] = (cdf[value] * 255.0) as u8;
        }
        
        Ok(output)
    }

    /// 簡易的なノイズ除去（3x3メディアンフィルタ）
    fn denoise_image(&self, image: &ImageBuffer<Luma<u8>, Vec<u8>>) -> Result<ImageBuffer<Luma<u8>, Vec<u8>>> {
        let width = image.width();
        let height = image.height();
        let mut output = image.clone();
        
        for y in 1..height-1 {
            for x in 1..width-1 {
                let mut pixels = Vec::new();
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        let px = (x as i32 + dx) as u32;
                        let py = (y as i32 + dy) as u32;
                        pixels.push(image.get_pixel(px, py)[0]);
                    }
                }
                pixels.sort();
                output.put_pixel(x, y, Luma([pixels[4]])); // 中央値を使用
            }
        }
        
        Ok(output)
    }

    /// シャープネス強化（アンシャープマスク）
    fn sharpen_image(&self, image: &ImageBuffer<Luma<u8>, Vec<u8>>) -> Result<ImageBuffer<Luma<u8>, Vec<u8>>> {
        let width = image.width();
        let height = image.height();
        let mut output = image.clone();
        
        // シャープニングカーネル
        let kernel = [
            [0.0, -1.0, 0.0],
            [-1.0, 5.0, -1.0],
            [0.0, -1.0, 0.0]
        ];
        
        for y in 1..height-1 {
            for x in 1..width-1 {
                let mut sum: f32 = 0.0;
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        let px = (x as i32 + dx) as u32;
                        let py = (y as i32 + dy) as u32;
                        let pixel_value = image.get_pixel(px, py)[0] as f32;
                        sum += pixel_value * kernel[(dy + 1) as usize][(dx + 1) as usize];
                    }
                }
                let value = sum.max(0.0).min(255.0) as u8;
                output.put_pixel(x, y, Luma([value]));
            }
        }
        
        Ok(output)
    }

    /// テキストの正規化（不要な空白や改行を削除）
    fn normalize_text(&self, text: &str) -> String {
        text.lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// OCR結果を表す構造体
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct OcrResult {
    /// 認識されたテキスト
    pub text: String,
    /// 認識の信頼度（0.0-1.0）
    pub confidence: f32,
    /// タイムスタンプ
    pub timestamp: std::time::SystemTime,
}

#[allow(dead_code)]
impl OcrResult {
    /// 新しいOCR結果を作成
    pub fn new(text: String, confidence: f32) -> Self {
        Self {
            text,
            confidence,
            timestamp: std::time::SystemTime::now(),
        }
    }
}