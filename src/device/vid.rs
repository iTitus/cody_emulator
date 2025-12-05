use crate::memory::Memory;

pub const CONTENT_WIDTH: u8 = 160;
pub const HIRES_WIDTH: u16 = 2 * CONTENT_WIDTH as u16;
pub const CONTENT_HEIGHT: u8 = 200;
pub const BORDER_X: u32 = 4;
pub const BORDER_Y: u32 = 8;
pub const WIDTH: u32 = HIRES_WIDTH as u32 + 2 * BORDER_X;
pub const HEIGHT: u32 = CONTENT_HEIGHT as u32 + 2 * BORDER_Y;

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Color {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

impl Default for Color {
    fn default() -> Self {
        Color::BLACK
    }
}

impl Color {
    pub const BLACK: Self = Self::rgb(0x000000);
    pub const WHITE: Self = Self::rgb(0xffffff);
    pub const RED: Self = Self::rgb(0xcc0000);
    pub const GREEN: Self = Self::rgb(0x33ff66);
    pub const BLUE: Self = Self::rgb(0x0d0066);
    pub const YELLOW: Self = Self::rgb(0xffe699);
    pub const PURPLE: Self = Self::rgb(0xcc0099);
    pub const CYAN: Self = Self::rgb(0x99ffd9);
    pub const ORANGE: Self = Self::rgb(0xffbf99);
    pub const BROWN: Self = Self::rgb(0xcc4d00);
    pub const GRAY: Self = Self::rgb(0x999999);
    pub const LIGHT_GRAY: Self = Self::rgb(0xcccccc);
    pub const DARK_GRAY: Self = Self::rgb(0x666666);
    pub const LIGHT_RED: Self = Self::rgb(0xff9999);
    pub const LIGHT_GREEN: Self = Self::rgb(0x99ffb3);
    pub const LIGHT_BLUE: Self = Self::rgb(0xa699ff);

    pub const PALETTE: [Self; 16] = [
        Self::BLACK,
        Self::WHITE,
        Self::RED,
        Self::CYAN,
        Self::PURPLE,
        Self::GREEN,
        Self::BLUE,
        Self::YELLOW,
        Self::ORANGE,
        Self::BROWN,
        Self::LIGHT_RED,
        Self::DARK_GRAY,
        Self::GRAY,
        Self::LIGHT_GREEN,
        Self::LIGHT_BLUE,
        Self::LIGHT_GRAY,
    ];

    const fn rgb(color: u32) -> Self {
        Self {
            r: ((color >> 16) & 0xFF) as u8,
            g: ((color >> 8) & 0xFF) as u8,
            b: (color & 0xFF) as u8,
            a: 255,
        }
    }
}

pub fn render_pixels<M: Memory>(memory: &mut M, raw_pixels: &mut [Color]) {
    let (
        disable_video,
        enable_v_scroll,
        enable_h_scroll,
        enable_row_effects,
        bitmap_mode,
        hires_mode,
    ) = {
        let control = memory.read_u8(0xD001);
        let hires_mode = (control & 0x20) != 0;
        (
            (control & 0x1) != 0,
            (control & 0x2) != 0 && !hires_mode,
            (control & 0x4) != 0 && !hires_mode,
            (control & 0x8) != 0,
            (control & 0x10) != 0,
            hires_mode,
        )
    };

    let color = memory.read_u8(0xD002);
    raw_pixels.fill(Color::PALETTE[(color & 0xF) as usize]); // fill with border color
    let color_memory_start = 0xA000u16.wrapping_add(0x400 * (color >> 4) as u16);

    if disable_video {
        return;
    }

    // these depend on the fine scrolling state
    let width = {
        let w = CONTENT_WIDTH as u16 - if enable_h_scroll { 2 * 4 } else { 0 };
        if hires_mode { w * 2 } else { w }
    };
    let height = CONTENT_HEIGHT - if enable_v_scroll { 8 } else { 0 };
    let border_x = BORDER_X as usize + if enable_h_scroll { 2 * 2 } else { 0 };
    let border_y = BORDER_Y as usize + if enable_v_scroll { 4 } else { 0 };

    let mut base = memory.read_u8(0xD003); // editable via 00 row effect
    let mut scroll = memory.read_u8(0xD004); // editable via 01 row effect
    let mut screen_colors = memory.read_u8(0xD005); // editable via 10 row effect
    let mut sprite = memory.read_u8(0xD006); // editable via 11 row effect

    let mut render_line =
        |y: u16, memory: &mut M, base: u8, scroll: u8, screen_colors: u8, sprite: u8| {
            let screen_memory_start = 0xA000u16.wrapping_add(0x400 * (base >> 4) as u16);
            let character_memory_start = 0xA000u16.wrapping_add(0x800 * (base & 0xF) as u16);
            let v_scroll_amount = if enable_v_scroll { scroll & 0x7 } else { 0 };
            let h_scroll_amount = if enable_h_scroll {
                (scroll >> 4) & 0x3
            } else {
                0
            };

            for x in 0..width {
                let scrolled_x = x + h_scroll_amount as u16;
                let scrolled_y = y + v_scroll_amount as u16;

                let tile_x = scrolled_x / if hires_mode { 8 } else { 4 };
                let tile_y = scrolled_y / 8;
                let tile_index = tile_y * 40 + tile_x;

                let in_tile_x = scrolled_x % if hires_mode { 8 } else { 4 };
                let in_tile_y = scrolled_y % 8;

                let palette_index = if hires_mode {
                    // background, fine scroll & sprites are disabled
                    let character_data_row = if bitmap_mode {
                        memory.read_u8(screen_memory_start.wrapping_add(8 * tile_index + in_tile_y))
                    } else {
                        let character =
                            memory.read_u8(screen_memory_start.wrapping_add(tile_index));
                        memory.read_u8(
                            character_memory_start.wrapping_add(8 * character as u16 + in_tile_y),
                        )
                    };
                    let local_colors = memory.read_u8(color_memory_start.wrapping_add(tile_index));
                    let character_data_pixel = (character_data_row >> (7 - in_tile_x)) & 0x1;
                    match character_data_pixel {
                        0 => local_colors & 0xF,
                        1 => local_colors >> 4,
                        _ => unreachable!(),
                    }
                } else {
                    // background
                    let character_data_row = if bitmap_mode {
                        memory.read_u8(screen_memory_start.wrapping_add(8 * tile_index + in_tile_y))
                    } else {
                        let character =
                            memory.read_u8(screen_memory_start.wrapping_add(tile_index));
                        memory.read_u8(
                            character_memory_start.wrapping_add(8 * character as u16 + in_tile_y),
                        )
                    };
                    let local_colors = memory.read_u8(color_memory_start.wrapping_add(tile_index));
                    let character_data_pixel = (character_data_row >> (2 * (3 - in_tile_x))) & 0x3;
                    let mut palette_index = match character_data_pixel {
                        0 => local_colors & 0xF,
                        1 => local_colors >> 4,
                        2 => screen_colors & 0xF,
                        3 => screen_colors >> 4,
                        _ => unreachable!(),
                    };

                    // sprites
                    const SPRITE_WIDTH: u8 = 12;
                    const SPRITE_HEIGHT: u8 = 21;

                    let sprite_common_color = sprite & 0xF;
                    let sprite_bank_start = 0xD080u16.wrapping_add(0x20 * ((sprite >> 4) as u16));
                    for sprite_index in 0..8 {
                        let sprite_data_start = sprite_bank_start.wrapping_add(4 * sprite_index);

                        let sprite_pos_x = memory.read_u8(sprite_data_start);
                        let min_x = (sprite_pos_x as i16) - (SPRITE_WIDTH as i16);
                        let max_x = sprite_pos_x as i16;
                        if !(min_x..max_x).contains(&(x as i16)) {
                            continue;
                        }

                        let sprite_pos_y = memory.read_u8(sprite_data_start.wrapping_add(1));
                        let min_y = (sprite_pos_y as i16) - (SPRITE_HEIGHT as i16);
                        let max_y = sprite_pos_y as i16;
                        if !(min_y..max_y).contains(&(y as i16)) {
                            continue;
                        }

                        let sprite_colors = memory.read_u8(sprite_data_start.wrapping_add(2));
                        let sprite_location = 0xA000u16.wrapping_add(
                            0x40 * memory.read_u8(sprite_data_start.wrapping_add(3)) as u16,
                        );

                        let in_sprite_x = (x as i16 - min_x) as u8;
                        let in_sprite_y = (y as i16 - min_y) as u8;
                        let sprite_pixel_index = in_sprite_y * SPRITE_WIDTH + in_sprite_x;
                        let sprite_byte_index = sprite_pixel_index / 4;
                        let sprite_byte_bit_shift = 2 * (3 - (sprite_pixel_index % 4));
                        let sprite_pixel_data = (memory
                            .read_u8(sprite_location.wrapping_add(sprite_byte_index as u16))
                            >> sprite_byte_bit_shift)
                            & 0x3;
                        match sprite_pixel_data {
                            0 => {} // transparent
                            1 => palette_index = sprite_colors & 0xF,
                            2 => palette_index = sprite_colors >> 4,
                            3 => palette_index = sprite_common_color,
                            _ => unreachable!(),
                        };
                    }

                    palette_index
                };

                let target_color = Color::PALETTE[palette_index as usize];
                if hires_mode {
                    let target_pos =
                        (y as usize + border_y) * WIDTH as usize + (x as usize + border_x);
                    raw_pixels[target_pos] = target_color;
                } else {
                    let target_pos =
                        (y as usize + border_y) * WIDTH as usize + (2 * x as usize + border_x);
                    raw_pixels[target_pos] = target_color;
                    raw_pixels[target_pos + 1] = target_color;
                }
            }
        };

    for y in 0..height {
        render_line(y as u16, memory, base, scroll, screen_colors, sprite);

        let tile_y = y / 8;
        let in_tile_y = y % 8;
        if enable_row_effects && in_tile_y == 0 {
            for effect_index in 0..32 {
                let effect_control = memory.read_u8(0xD040 + effect_index);
                if effect_control & 0x80 == 0 {
                    continue;
                }
                let row = effect_control & 0x1F;
                if row != tile_y {
                    continue;
                }
                let destination = (effect_control >> 5) & 0x3;
                let effect_data = memory.read_u8(0xD060 + effect_index);
                match destination {
                    0 => base = effect_data,
                    1 => scroll = effect_data,
                    2 => screen_colors = effect_data,
                    3 => sprite = effect_data,
                    _ => unreachable!(),
                }
            }
        }
    }
}
