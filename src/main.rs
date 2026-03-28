#![cfg_attr(target_os = "none", no_std)]
#![no_main]

#[macro_use]
mod nadk;
mod grid;

use nadk::display::{Color565, push_rect};
use nadk::keyboard::Key;
use nadk::utils::wait_ok_released;
use grid::Grid;

use crate::nadk::display::ScreenRect;
use crate::nadk::keyboard::InputManager;
use crate::nadk::time::get_current_time_seconds;

const SCALE_X: i32 = 320 / grid::GRID_WIDTH;
const SCALE_Y: i32 = 240 / grid::GRID_HEIGHT;

const POWER_MULT: f32 = 0.05;

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

configure_app!(b"FluidSim\0", 9, "../target/icon.nwi", 745);

pub fn fast_sqrt(x: f32) -> f32 {
    let i = x.to_bits();
    let i = 0x5f3759df - (i >> 1);
    let y = f32::from_bits(i);
    let y = y * (1.5 - 0.5 * x * y * y);
    y * x // 1/sqrt(x) * x = sqrt(x)
}

setup_allocator!();

#[unsafe(no_mangle)]
fn main() {
    init_heap!();
    wait_ok_released();

    let mut grid = Grid::new();
    let mut cell_buffer = [
        Color565::from_rgb888(0, 0, 0);
        SCALE_X as usize * SCALE_Y as usize
    ];

    let mut current_color_idx = 0;
    let mut constant_flow = false;

    let mut im = InputManager::new();

    loop {
        im.scan();
        let dt = 0.1;

        if im.is_just_released(Key::Back) {
            break;
        }
        if im.is_just_pressed(Key::Backspace) {
            grid = Grid::new();
        }
        if im.is_just_pressed(Key::Exe) {
            constant_flow = !constant_flow;
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

        // Add density in the center
        if im.is_keydown(Key::Ok) || get_current_time_seconds() < 0.5 || constant_flow {
            let curr_color = FLUID_COLORS[current_color_idx]
                .get_components();

            let r = curr_color.0 as f32 / 31.0;
            let g = curr_color.1 as f32 / 63.0;
            let b = curr_color.2 as f32 / 31.0;

            grid.spawn_dye(r, g, b);
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
        fx *= POWER_MULT;
        fy *= POWER_MULT;

        if fx != 0.0 || fy != 0.0 {
            grid.apply_circular_force(
                grid::GRID_WIDTH as f32 / 2.0,
                grid::GRID_HEIGHT as f32 / 2.0,
                fx,
                fy,
                dt
            );
        }

        grid.step(dt);

        // Render density into cell_buffer
        for gy in 0..grid::GRID_HEIGHT {
            for gx in 0..grid::GRID_WIDTH {
                let color = grid.get_color(grid::idx(gx as usize, gy as usize));

                for i in 0..(SCALE_X * SCALE_Y) as usize {
                    cell_buffer[i] = color;
                }

                let rect = ScreenRect {
                    x: (gx * SCALE_X) as u16,
                    y: (gy * SCALE_Y) as u16,
                    width: SCALE_X as u16,
                    height: SCALE_Y as u16,
                };

                push_rect(rect, &cell_buffer);
            }
        }
    }
}
