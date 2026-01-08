#![allow(unused)]

use crate::geom::lerp;
use serde::{Deserialize, Serialize};

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Colour {
    Grey(u8),
    Rgb(u8, u8, u8),
}

impl Colour {
    pub fn grey(&self) -> u8 {
        match *self {
            Colour::Grey(level) => level,
            Colour::Rgb(red, green, blue) => {
                (red as f32 * 0.2126 + green as f32 * 0.7152 + blue as f32 * 0.0722) as u8
            }
        }
    }

    pub fn rgb(&self) -> [u8; 3] {
        match *self {
            Colour::Grey(level) => [level; 3],
            Colour::Rgb(red, green, blue) => [red, green, blue],
        }
    }

    pub fn from_rgb(rgb: &[u8]) -> Colour {
        Colour::Rgb(rgb[0], rgb[1], rgb[2])
    }

    pub fn apply<F>(&self, f: F) -> Colour
    where
        F: Fn(u8) -> u8,
    {
        match *self {
            Colour::Grey(level) => Colour::Grey(f(level)),
            Colour::Rgb(red, green, blue) => Colour::Rgb(f(red), f(green), f(blue)),
        }
    }

    pub fn lerp(&self, color: Colour, alpha: f32) -> Colour {
        match (*self, color) {
            (Colour::Grey(l1), Colour::Grey(l2)) => {
                Colour::Grey(lerp(l1 as f32, l2 as f32, alpha) as u8)
            }
            (Colour::Rgb(red, green, blue), Colour::Grey(level)) => Colour::Rgb(
                lerp(red as f32, level as f32, alpha) as u8,
                lerp(green as f32, level as f32, alpha) as u8,
                lerp(blue as f32, level as f32, alpha) as u8,
            ),
            (Colour::Grey(level), Colour::Rgb(red, green, blue)) => Colour::Rgb(
                lerp(level as f32, red as f32, alpha) as u8,
                lerp(level as f32, green as f32, alpha) as u8,
                lerp(level as f32, blue as f32, alpha) as u8,
            ),
            (Colour::Rgb(r1, g1, b1), Colour::Rgb(r2, g2, b2)) => Colour::Rgb(
                lerp(r1 as f32, r2 as f32, alpha) as u8,
                lerp(g1 as f32, g2 as f32, alpha) as u8,
                lerp(b1 as f32, b2 as f32, alpha) as u8,
            ),
        }
    }

    pub fn invert(&mut self) {
        match self {
            Colour::Grey(level) => *level = 255 - *level,
            Colour::Rgb(red, green, blue) => {
                *red = 255 - *red;
                *green = 255 - *green;
                *blue = 255 - *blue;
            }
        }
    }

    pub fn shift(&mut self, drift: u8) {
        match self {
            Colour::Grey(level) => *level = level.saturating_sub(drift),
            Colour::Rgb(red, green, blue) => {
                *red = red.saturating_sub(drift);
                *green = green.saturating_sub(drift);
                *blue = blue.saturating_sub(drift);
            }
        }
    }
}

macro_rules! grey {
    ($a:expr) => {
        $crate::colour::Colour::Grey($a)
    };
}

pub const GREY00: Colour = grey!(0x00);
pub const GREY01: Colour = grey!(0x11);
pub const GREY02: Colour = grey!(0x22);
pub const GREY03: Colour = grey!(0x33);
pub const GREY04: Colour = grey!(0x44);
pub const GREY05: Colour = grey!(0x55);
pub const GREY06: Colour = grey!(0x66);
pub const GREY07: Colour = grey!(0x77);
pub const GREY08: Colour = grey!(0x88);
pub const GREY09: Colour = grey!(0x99);
pub const GREY10: Colour = grey!(0xAA);
pub const GREY11: Colour = grey!(0xBB);
pub const GREY12: Colour = grey!(0xCC);
pub const GREY13: Colour = grey!(0xDD);
pub const GREY14: Colour = grey!(0xEE);
pub const GREY15: Colour = grey!(0xFF);

pub const BLACK: Colour = GREY00;
pub const WHITE: Colour = GREY15;

pub const TEXT_NORMAL: [Colour; 3] = [WHITE, BLACK, GREY08];
pub const TEXT_BUMP_SMALL: [Colour; 3] = [GREY14, BLACK, GREY07];
pub const TEXT_BUMP_LARGE: [Colour; 3] = [GREY11, BLACK, BLACK];

pub const TEXT_INVERTED_SOFT: [Colour; 3] = [GREY05, WHITE, WHITE];
pub const TEXT_INVERTED_HARD: [Colour; 3] = [BLACK, WHITE, GREY06];

pub const SEPARATOR_NORMAL: Colour = GREY10;
pub const SEPARATOR_STRONG: Colour = GREY07;

pub const KEYBOARD_BG: Colour = GREY12;
pub const BATTERY_FILL: Colour = GREY12;
pub const READING_PROGRESS: Colour = GREY07;

pub const PROGRESS_FULL: Colour = GREY05;
pub const PROGRESS_EMPTY: Colour = GREY13;
pub const PROGRESS_VALUE: Colour = GREY06;
