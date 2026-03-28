#![cfg_attr(target_os = "none", no_std)]
#![no_main]

#[macro_use]
mod nadk;
mod grid;

use nadk::display::{Color565, SCREEN_RECT, push_rect};
use nadk::keyboard::{Key, KeyboardState};
use nadk::utils::wait_ok_released;
use grid::Grid;

const SCALE_X: usize = 320 / grid::GRID_WIDTH;
const SCALE_Y: usize = 240 / grid::GRID_HEIGHT;

configure_app!(b"FluidSim\0", 10, "../target/icon.nwi", 745);

setup_allocator!();

pub fn fast_sqrt(x: f32) -> f32 {
    let i = x.to_bits();
    let i = 0x5f3759df - (i >> 1);
    let y = f32::from_bits(i);
    let y = y * (1.5 - 0.5 * x * y * y);
    y * x // 1/sqrt(x) * x = sqrt(x)
}

fn density_to_color(d: f32) -> Color565 {
    let d = d.clamp(0.0, 1.0);
    let r = (d * 0.0) as u16;
    let g = (d * 255.0) as u16;
    let b = (d * 255.0) as u16;
    Color565::from_rgb888(r, g, b)
}

#[unsafe(no_mangle)]
fn main() {
    init_heap!();
    wait_ok_released();

    let mut grid = Grid::new();
    let mut framebuffer = [Color565::from_rgb888(0, 0, 0); 320 * 240];

    let viscosity = 0.0001;
    let diffusion = 0.0001;

    let circle_cx = grid::GRID_WIDTH as f32 / 2.0;
    let circle_cy = grid::GRID_HEIGHT as f32 / 2.0;
    let circle_r_outer = 20.0;
    let circle_r_inner = 10.0; // fade start
    let circle_outer_sq = circle_r_outer * circle_r_outer;
    let circle_inner_sq = circle_r_inner * circle_r_inner;
    let circle_max_dens = 0.16;

    let power_mult = 0.075;

    loop {
        let kb = KeyboardState::scan();
        let dt = 0.1;

        if kb.key_down(Key::Ok) {
            break;
        }

        // Clear sources from last frame
        grid.clear_sources();

        // Add density in the center
        if kb.key_down(Key::Back) {
            for y in 1..=grid::GRID_HEIGHT as i32 {
                for x in 1..=grid::GRID_WIDTH as i32 {
                    let dx = x as f32 - circle_cx;
                    let dy = y as f32 - circle_cy;
                    let dist_sq = dx * dx + dy * dy;

                    let idx = grid::idx(x as usize, y as usize);

                    if dist_sq <= circle_inner_sq {
                        grid.density_prev[idx] = circle_max_dens;
                    } else if dist_sq <= circle_outer_sq {
                        let fraction = (circle_outer_sq - dist_sq) / (circle_outer_sq - circle_inner_sq);
                        grid.density_prev[idx] = fraction * circle_max_dens;
                    }
                }
            }
        }

        let mut fx = 0.0;
        let mut fy = 0.0;

        if kb.key_down(Key::Left)  { fx -= 1.0; }
        if kb.key_down(Key::Right) { fx += 1.0; }
        if kb.key_down(Key::Up)    { fy -= 1.0; }
        if kb.key_down(Key::Down)  { fy += 1.0; }

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
                circle_r_outer,
                fx,
                fy,
                dt
            );
        }

        grid.step(viscosity, diffusion, dt);

        // Render density into framebuffer
        for gy in 0..grid::GRID_HEIGHT {
            for gx in 0..grid::GRID_WIDTH {
                let d = grid.density[grid::idx(gx + 1, gy + 1)];
                let color = density_to_color(d);

                // Fill SCALE_X × SCALE_Y block of screen pixels
                for py in 0..SCALE_Y {
                    for px in 0..SCALE_X {
                        let sx = gx * SCALE_X + px;
                        let sy = gy * SCALE_Y + py;
                        framebuffer[sy * 320 + sx] = color;
                    }
                }
            }
        }

        push_rect(SCREEN_RECT, &framebuffer);
    }
}
