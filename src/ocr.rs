// OCR（光学文字認識）機能の実装
use anyhow::{Result, Context};
use image::DynamicImage;
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

        // フォールバック方式でOCRを実行（EXC_BAD_ACCESS回避）
        self.recognize_with_fallback(&processed_image)
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
        let tesseract = Tesseract::new(None, Some("jpn"))
            .context("Tesseract（日本語）の初期化に失敗しました")?;
        
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

        let tesseract = Tesseract::new(None, Some("jpn"))
            .context("簡素Tesseract（日本語）の初期化に失敗しました")?;
        
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

        // 安全なグレースケール変換
        processed = processed.grayscale();

        // 軽微なコントラスト調整
        processed = processed.brighten(5); // 10から5に削減

        // より保守的なリサイズ（メモリ使用量削減）
        if processed.width() < 400 { // 800から400に削減
            let scale_factor = 400.0 / processed.width() as f32;
            
            // スケールファクターの上限設定（メモリ保護）
            let safe_scale_factor = scale_factor.min(3.0);
            
            let new_width = (processed.width() as f32 * safe_scale_factor) as u32;
            let new_height = (processed.height() as f32 * safe_scale_factor) as u32;
            
            // 最終サイズチェック
            if new_width <= 2048 && new_height <= 2048 {
                processed = processed.resize(new_width, new_height, imageops::FilterType::Nearest); // Lanczos3からNearestに変更
            }
        }

        log::debug!("画像前処理完了: {}x{}", processed.width(), processed.height());
        Ok(processed)
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