// 画面キャプチャ機能の実装
use anyhow::{Result, Context};
use image::{DynamicImage, RgbaImage};
use screenshots::Screen;
use std::time::Instant;
use serde::{Deserialize, Serialize};

/// 画面上の指定領域をキャプチャする構造体
#[derive(Debug, Clone)]
pub struct ScreenCapture {
    /// キャプチャする領域の情報
    pub region: CaptureRegion,
}

/// キャプチャ領域を表す構造体
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CaptureRegion {
    /// 左上のX座標
    pub x: i32,
    /// 左上のY座標
    pub y: i32,
    /// 幅
    pub width: u32,
    /// 高さ
    pub height: u32,
}

impl ScreenCapture {
    /// 新しいScreenCaptureインスタンスを作成
    pub fn new(region: CaptureRegion) -> Self {
        Self { region }
    }

    /// 指定された領域の画面をキャプチャ
    pub fn capture(&self) -> Result<DynamicImage> {
        let start = Instant::now();
        
        // プライマリスクリーンを取得
        let screens = Screen::all()
            .context("スクリーンの取得に失敗しました")?;
        
        let screen = screens.first()
            .context("プライマリスクリーンが見つかりません")?;

        // 座標とサイズのバリデーション（EXC_BAD_ACCESS回避）
        if self.region.x < 0 || self.region.y < 0 {
            return Err(anyhow::anyhow!("座標が負の値です: x={}, y={}", self.region.x, self.region.y));
        }
        
        if self.region.width == 0 || self.region.height == 0 {
            return Err(anyhow::anyhow!("サイズが無効です: width={}, height={}", self.region.width, self.region.height));
        }

        // 安全なサイズ制限（メモリ保護）
        if self.region.width > 2048 || self.region.height > 2048 {
            return Err(anyhow::anyhow!("キャプチャサイズが大きすぎます: {}x{}", self.region.width, self.region.height));
        }

        // 画面境界チェック（可能な範囲で）
        if self.region.x > 5120 || self.region.y > 2880 { // 一般的な大画面の最大解像度考慮
            return Err(anyhow::anyhow!("座標が画面範囲を超えています: x={}, y={}", self.region.x, self.region.y));
        }

        // 指定領域をキャプチャ
        let image = screen.capture_area(
            self.region.x,
            self.region.y,
            self.region.width,
            self.region.height,
        ).context("画面のキャプチャに失敗しました")?;

        // DynamicImageに変換
        let rgba_image = self.screen_image_to_rgba(image)?;
        let dynamic_image = DynamicImage::ImageRgba8(rgba_image);

        log::debug!("キャプチャ完了: {:?}", start.elapsed());

        Ok(dynamic_image)
    }

    /// ScreenImageをRgbaImageに変換
    fn screen_image_to_rgba(&self, screen_img: image::RgbaImage) -> Result<RgbaImage> {
        Ok(screen_img)
    }

    /// 全画面をキャプチャ（領域選択用）
    #[allow(dead_code)]
    pub fn capture_full_screen() -> Result<DynamicImage> {
        let screens = Screen::all()
            .context("スクリーンの取得に失敗しました")?;
        
        let screen = screens.first()
            .context("プライマリスクリーンが見つかりません")?;

        let image = screen.capture()
            .context("全画面のキャプチャに失敗しました")?;

        // DynamicImageに変換
        Ok(DynamicImage::ImageRgba8(image))
    }
}