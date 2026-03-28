#![cfg_attr(target_os = "none", no_std)]
#![no_main]

#[macro_use]
mod nadk;
mod grid;

use nadk::display::{Color565, SCREEN_RECT, push_rect};
use nadk::keyboard::Key;
use nadk::utils::wait_ok_released;
use grid::Grid;

use crate::nadk::keyboard::InputManager;
use crate::nadk::time::get_current_time_seconds;

const SCALE_X: usize = 320 / grid::GRID_WIDTH;
const SCALE_Y: usize = 240 / grid::GRID_HEIGHT;

const CIRCLE_CX: f32       = grid::GRID_WIDTH as f32 / 2.0;
const CIRCLE_CY: f32       = grid::GRID_HEIGHT as f32 / 2.0;
const CIRCLE_R_OUTER: f32  = 20.0;
const CIRCLE_R_INNER: f32  = 10.0; // fade start
const CIRCLE_OUTER_SQ: f32 = CIRCLE_R_OUTER * CIRCLE_R_OUTER;
const CIRCLE_INNER_SQ: f32 = CIRCLE_R_INNER * CIRCLE_R_INNER;
const CIRCLE_MAX_DENS: f32 = 0.2;

const FLUID_COLORS: [Color565; 10] = [
    Color565::from_rgb888(0,   255, 255), // Electric Cyan
    Color565::from_rgb888(0,   255, 0),   // Neon Green
    Color565::from_rgb888(255, 0,   127), // Hot Pink
    Color565::from_rgb888(255, 128, 0),   // Vivid Orange
    Color565::from_rgb888(255, 255, 0),   // Solar Yellow
    Color565::from_rgb888(178, 0,   255), // Electric Purple
    Color565::from_rgb888(51,  102, 255), // Ultramarine
    Color565::from_rgb888(255, 0,   51),  // Bright Crimson
    Color565::from_rgb888(0,   255, 128), // Spring Green
    Color565::from_rgb888(255, 255, 255), // Plasma White
];

configure_app!(b"FluidSim\0", 10, "../target/icon.nwi", 745);

setup_allocator!();

pub fn fast_sqrt(x: f32) -> f32 {
    let i = x.to_bits();
    let i = 0x5f3759df - (i >> 1);
    let y = f32::from_bits(i);
    let y = y * (1.5 - 0.5 * x * y * y);
    y * x // 1/sqrt(x) * x = sqrt(x)
}

fn density_to_color(r: f32, g: f32, b: f32) -> Color565 {
    let r = r.clamp(0.0, 1.0);
    let g = g.clamp(0.0, 1.0);
    let b = b.clamp(0.0, 1.0);

    let r_u8 = (r * 255.0) as u16;
    let g_u8 = (g * 255.0) as u16;
    let b_u8 = (b * 255.0) as u16;

    Color565::from_rgb888(r_u8, g_u8, b_u8)
}

fn spawn_density(grid: &mut Grid, r: f32, g: f32, b: f32) {
    for y in 1..=grid::GRID_HEIGHT as i32 {
        for x in 1..=grid::GRID_WIDTH as i32 {
            let dx = x as f32 - CIRCLE_CX;
            let dy = y as f32 - CIRCLE_CY;
            let dist_sq = dx * dx + dy * dy;

            if dist_sq <= CIRCLE_OUTER_SQ {
                let idx = grid::idx(x as usize, y as usize);

                // Calculate intensity based on distance
                let falloff = if dist_sq <= CIRCLE_INNER_SQ {
                    1.0
                } else {
                    (CIRCLE_OUTER_SQ - dist_sq) / (CIRCLE_OUTER_SQ - CIRCLE_INNER_SQ)
                };

                let amount = falloff * CIRCLE_MAX_DENS;

                grid.r_prev[idx] += amount * r;
                grid.g_prev[idx] += amount * g;
                grid.b_prev[idx] += amount * b;
            }
        }
    }
}

#[unsafe(no_mangle)]
fn main() {
    init_heap!();
    wait_ok_released();

    let mut grid = Grid::new();
    let mut framebuffer = [Color565::from_rgb888(0, 0, 0); 320 * 240];

    let viscosity = 0.0001;
    let diffusion = 0.0001;

    let power_mult = 0.075;

    let mut current_color_idx = 0;
    let mut constant_stream = false;

    let mut im = InputManager::new();

    loop {
        im.scan();
        let dt = 0.1;

        if im.is_keydown(Key::Ok) {
            break;
        }
        if im.is_keydown(Key::Backspace) {
            grid = Grid::new();
        }
        if im.is_just_pressed(Key::Exe) {
            constant_stream = !constant_stream;
        }

        // Switch colors
        if im.is_just_pressed(Key::Zero)  { current_color_idx = 0; }
        if im.is_just_pressed(Key::One)   { current_color_idx = 1; }
        if im.is_just_pressed(Key::Two)   { current_color_idx = 2; }
        if im.is_just_pressed(Key::Three) { current_color_idx = 3; }
        if im.is_just_pressed(Key::Four)  { current_color_idx = 4; }
        if im.is_just_pressed(Key::Five)  { current_color_idx = 5; }
        if im.is_just_pressed(Key::Six)   { current_color_idx = 6; }
        if im.is_just_pressed(Key::Seven) { current_color_idx = 7; }
        if im.is_just_pressed(Key::Eight) { current_color_idx = 8; }
        if im.is_just_pressed(Key::Nine)  { current_color_idx = 9; }

        // Clear sources from last frame
        grid.clear_sources();

        // Add density in the center
        if im.is_keydown(Key::Back) || get_current_time_seconds() < 1.0 || constant_stream {
            let curr_color = FLUID_COLORS[current_color_idx]
                .get_components();

            let r = curr_color.0 as f32 / 31.0;
            let g = curr_color.1 as f32 / 63.0;
            let b = curr_color.2 as f32 / 31.0;

            spawn_density(&mut grid, r, g, b);
        }

        let mut fx = 0.0;
        let mut fy = 0.0;

        if im.is_keydown(Key::Left)  { fx -= 1.0; }
        if im.is_keydown(Key::Right) { fx += 1.0; }
        if im.is_keydown(Key::Up)    { fy -= 1.0; }
        if im.is_keydown(Key::Down)  { fy += 1.0; }

        let mag = fast_sqrt(fx*fx + fy*fy);
        if mag > 0.0 {
            fx /= mag;
            fy /= mag;
        }
        fx *= power_mult;
        fy *= power_mult;

        if fx != 0.0 || fy != 0.0 {
            grid.apply_circular_source(
                grid::GRID_WIDTH as f32 / 2.0,
                grid::GRID_HEIGHT as f32 / 2.0,
                CIRCLE_R_OUTER,
                fx,
                fy,
                dt
            );
        }

        grid.step(viscosity, diffusion, dt);

        // Render density into framebuffer
        for gy in 0..grid::GRID_HEIGHT {
            for gx in 0..grid::GRID_WIDTH {
                let idx = grid::idx(gx + 1, gy + 1);
                let r_val = grid.r[idx];
                let g_val = grid.g[idx];
                let b_val = grid.b[idx];

                let color = density_to_color(r_val, g_val, b_val);

                // Fill SCALE_X × SCALE_Y block of screen pixels
                for py in 0..SCALE_Y {
                    let sy_offset = (gy * SCALE_Y + py) * 320;
                    for px in 0..SCALE_X {
                        let sx = gx * SCALE_X + px;
                        framebuffer[sy_offset + sx] = color;
                    }
                }
            }
        }

        push_rect(SCREEN_RECT, &framebuffer);
    }
}
