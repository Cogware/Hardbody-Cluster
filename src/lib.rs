#![no_std]
extern crate alloc;

use alloc::format;
use embedded_graphics::{
    mono_font::{ascii::FONT_10X20, MonoTextStyleBuilder},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{Line, PrimitiveStyle, Rectangle},
    text::{Baseline, Text},
};
use micromath::F32Ext;

pub fn draw_fuel_gauge<D>(
    display: &mut D,
    adc: i16,
    batt_adc: i16,
    v33_adc: i16,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = BinaryColor>,
{
    // Hardware constants
    const R_PULL: f32 = 1_000.0; // 1 kΩ pull-up to 3.3V
    const R_FULL: f32 = 3.8; // sender ≈ full
    const R_EMPTY: f32 = 93.0; // sender ≈ empty

    // ADS1115 transfer (for battery volts display)
    const FS_V: f32 = 4.096; // ±4.096 V PGA
    const ADC_MAX: f32 = 32767.0;

    // Battery divider
    const R1: f32 = 100_000.0; // top
    const R2: f32 = 22_000.0; // bottom

    // --- Ratiometric % fuel ---
    // ratio = V_sense / V_3v3 = code_sense / code_v33
    let v33 = (v33_adc.max(1) as f32).min(ADC_MAX); // avoid /0, clamp top
    let mut ratio = (adc.max(0) as f32).min(ADC_MAX) / v33;
    if ratio < 0.0 {
        ratio = 0.0;
    }
    if ratio > 0.999_999 {
        ratio = 0.999_999;
    }

    // Endpoints in ratio space (independent of rail voltage)
    let ratio_full = R_FULL / (R_PULL + R_FULL);
    let ratio_empty = R_EMPTY / (R_PULL + R_EMPTY);

    // Map ratio → 0..100% (full at low R)
    let mut pct_f = (ratio_empty - ratio) / (ratio_empty - ratio_full);
    if pct_f < 0.0 {
        pct_f = 0.0;
    }
    if pct_f > 1.0 {
        pct_f = 1.0;
    }
    let pct: u8 = (pct_f * 100.0 + 0.5) as u8;

    let v_batt_sense = (batt_adc.max(0) as f32).min(ADC_MAX) * FS_V / ADC_MAX; // volts at A2
    let batt_v = v_batt_sense * (R1 + R2) / R2; // divider scaled

    let text_style = MonoTextStyleBuilder::new()
        .font(&FONT_10X20)
        .text_color(BinaryColor::On)
        .build();

    let start = Point::new(15, 5);
    let end = Point::new(113, 5);
    let bar_h = 6;
    let tick_h = 8;
    let ptr_top = start.y + bar_h + 4;
    let ptr_len = 12;
    let w = (end.x - start.x) as u32;

    Rectangle::new(start, Size::new(w, bar_h as u32))
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
        .draw(display)?;

    for i in 0..=4 {
        let x = start.x + (w as i32 * i) / 4;
        let t0 = Point::new(x, start.y - (tick_h / 2));
        let t1 = Point::new(x, start.y + bar_h + (tick_h / 2));
        Line::new(t0, t1)
            .into_styled(PrimitiveStyle::with_stroke(BinaryColor::Off, 2))
            .draw(display)?;
    }

    let x_pos = start.x + ((pct as u32 * w) / 100) as i32;
    Line::new(
        Point::new(x_pos, ptr_top),
        Point::new(x_pos, ptr_top + ptr_len),
    )
    .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 4))
    .draw(display)?;

    Text::with_baseline("E", Point::new(0, 2), text_style, Baseline::Top).draw(display)?;
    Text::with_baseline("F", Point::new(118, 2), text_style, Baseline::Top).draw(display)?;
    Text::with_baseline(
        &format!("{}%", pct),
        Point::new(44, 30),
        text_style,
        Baseline::Top,
    )
    .draw(display)?;

    Text::with_baseline(
        &format!("{:.2}V", batt_v),
        Point::new(44, 45),
        text_style,
        Baseline::Top,
    )
    .draw(display)?;

    Ok(())
}

/// Temperature gauge with ratiometric correction (A3 = 3.3V)
/// - `adc`     = thermistor (A0) raw i16 (0..32767 expected)
/// - `v33_adc` = 3.3V rail (A3) raw i16
pub fn draw_temp_gauge<D>(display: &mut D, adc: i16, v33_adc: i16) -> Result<(), D::Error>
where
    D: DrawTarget<Color = BinaryColor>,
{
    // Constants
    const R_PULL: f32 = 1_000.0; // 1 k to 3.3V
    const BETA: f32 = 3962.0;
    const R25: f32 = 325.0;
    const MIN_F: f32 = 120.0;
    const MAX_F: f32 = 270.0;

    // Ratiometric resistance: ratio = V_sense / V_3v3 = code / v33_code
    let v33 = v33_adc.max(1) as f32; // avoid /0
    let mut ratio = (adc.max(0) as f32) / v33;
    if ratio < 1e-6 {
        ratio = 1e-6;
    } // avoid 0 → ln issues
    if ratio > 0.999_999 {
        ratio = 0.999_999;
    }

    // R_th = R_pull * ratio / (1 - ratio)
    let r_th = R_PULL * ratio / (1.0 - ratio);

    let inv_t = 1.0 / 298.15 + (r_th / R25).ln() / BETA;
    let t_k = 1.0 / inv_t;
    let t_c = t_k - 273.15;
    let t_f = t_c * 1.8 + 32.0;

    let text_style = MonoTextStyleBuilder::new()
        .font(&FONT_10X20)
        .text_color(BinaryColor::On)
        .build();

    let start = Point::new(15, 5);
    let end = Point::new(113, 5);
    let bar_h = 6;
    let ptr_top = start.y + bar_h + 4;
    let ptr_len = 12;
    let w = (end.x - start.x) as u32;

    Rectangle::new(start, Size::new(w, bar_h as u32))
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
        .draw(display)?;

    let pct = ((t_f - MIN_F) / (MAX_F - MIN_F)).clamp(0.0, 1.0);
    let x_pos = start.x + ((pct * w as f32).round() as i32);
    Line::new(
        Point::new(x_pos, ptr_top),
        Point::new(x_pos, ptr_top + ptr_len),
    )
    .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 4))
    .draw(display)?;

    let bar_thickness = 6;
    let tick_height = 8;

    let x = 30;
    Line::new(
        Point::new(x, start.y - (tick_height / 2)),
        Point::new(x, start.y + bar_thickness + (tick_height / 2)),
    )
    .into_styled(PrimitiveStyle::with_stroke(BinaryColor::Off, 3))
    .draw(display)?;

    let x = 98;
    Line::new(
        Point::new(x, start.y - (tick_height / 2)),
        Point::new(x, start.y + bar_thickness + (tick_height / 2)),
    )
    .into_styled(PrimitiveStyle::with_stroke(BinaryColor::Off, 3))
    .draw(display)?;

    Text::with_baseline("C", Point::new(0, 2), text_style, Baseline::Top).draw(display)?;
    Text::with_baseline("H", Point::new(118, 2), text_style, Baseline::Top).draw(display)?;
    Text::with_baseline(
        &format!("{:.0}F", t_f),
        Point::new(44, 30),
        text_style,
        Baseline::Top,
    )
    .draw(display)?;

    Text::with_baseline(
        &format!("{:.2}V", code_to_volts_f32(v33)),
        Point::new(44, 45),
        text_style,
        Baseline::Top,
    )
    .draw(display)?;

    Ok(())
}

fn code_to_volts_f32(code: f32) -> f32 {
    const FS_V: f32 = 4.096;
    const ADC_MAX: f32 = 32767.0;
    let c = code.max(0.0).min(ADC_MAX);
    c * FS_V / ADC_MAX
}
