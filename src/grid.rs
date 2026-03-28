// Possible values: 320x240, 160x120, 80x60, 64x48
pub const GRID_WIDTH:  i32 = 64;
pub const GRID_HEIGHT: i32 = 48;
pub const GRID_WIDTH_WITH_BOUNDARY:  usize = GRID_WIDTH  as usize + 2;
pub const GRID_HEIGHT_WITH_BOUNDARY: usize = GRID_HEIGHT as usize + 2;
pub const GRID_WITH_BOUNDARY_SIZE:   usize = GRID_WIDTH_WITH_BOUNDARY
                                           * GRID_HEIGHT_WITH_BOUNDARY;

const SIM_STEPS: usize = 10;

// Maximum representable velocity magnitude.
// smaller = more precision.
const VEL_MAX: f32 = 10.0;

// ------------------------------------------------------------------ //
//  index helper                                                        //
// ------------------------------------------------------------------ //

#[inline(always)]
pub const fn idx(x: usize, y: usize) -> usize {
    x + GRID_WIDTH_WITH_BOUNDARY * y
}

// ------------------------------------------------------------------ //
//  fixed-point encode / decode                                         //
// ------------------------------------------------------------------ //

/// Encode a velocity value in [-VEL_MAX, +VEL_MAX] to u16.
#[inline(always)]
fn enc_vel(v: f32) -> u16 {
    let norm = (v / VEL_MAX).clamp(-1.0, 1.0);
    (norm * 32767.0 + 32768.5) as u16
}

/// Decode a u16 velocity back to f32.
#[inline(always)]
fn dec_vel(raw: u16) -> f32 {
    (raw as f32 - 32768.0) / 32767.0 * VEL_MAX
}

/// Encode a density value in [0, 1] to u16.
#[inline(always)]
fn enc_den(v: f32) -> u16 {
    (v.clamp(0.0, 1.0) * 65535.0 + 0.5) as u16
}

/// Decode a u16 density back to f32.
#[inline(always)]
fn dec_den(raw: u16) -> f32 {
    raw as f32 / 65535.0
}

// ------------------------------------------------------------------ //
//  Grid                                                                //
// ------------------------------------------------------------------ //

pub struct Grid {
    /// Velocity X — fixed-point, signed, bias 0x8000
    pub u: [u16; GRID_WITH_BOUNDARY_SIZE],
    /// Velocity Y — fixed-point, signed, bias 0x8000
    pub v: [u16; GRID_WITH_BOUNDARY_SIZE],

    /// Dye channels — fixed-point, unsigned
    pub r: [u16; GRID_WITH_BOUNDARY_SIZE],
    pub g: [u16; GRID_WITH_BOUNDARY_SIZE],
    pub b: [u16; GRID_WITH_BOUNDARY_SIZE],
}

// Zero velocity encodes as 0x8000 (midpoint), zero density as 0x0000.
const ZERO_VEL: u16 = 0x8000;
const ZERO_DEN: u16 = 0x0000;

impl Grid {
    pub fn new() -> Self {
        Self {
            u: [ZERO_VEL; GRID_WITH_BOUNDARY_SIZE],
            v: [ZERO_VEL; GRID_WITH_BOUNDARY_SIZE],
            r: [ZERO_DEN; GRID_WITH_BOUNDARY_SIZE],
            g: [ZERO_DEN; GRID_WITH_BOUNDARY_SIZE],
            b: [ZERO_DEN; GRID_WITH_BOUNDARY_SIZE],
        }
    }

    // ------------------------------------------------------------------ //
    //  boundary                                                            //
    // ------------------------------------------------------------------ //

    fn set_bnd_vel(b: i32, field: &mut [u16; GRID_WITH_BOUNDARY_SIZE]) {
        for y in 1..=GRID_HEIGHT as usize {
            let inner_l = dec_vel(field[idx(1,           y)]);
            let inner_r = dec_vel(field[idx(GRID_WIDTH as usize, y)]);
            field[idx(0,                       y)] = enc_vel(if b == 1 { -inner_l } else { inner_l });
            field[idx(GRID_WIDTH as usize + 1, y)] = enc_vel(if b == 1 { -inner_r } else { inner_r });
        }
        for x in 1..=GRID_WIDTH as usize {
            let inner_t = dec_vel(field[idx(x, 1)]);
            let inner_b = dec_vel(field[idx(x, GRID_HEIGHT as usize)]);
            field[idx(x, 0                       )] = enc_vel(if b == 2 { -inner_t } else { inner_t });
            field[idx(x, GRID_HEIGHT as usize + 1)] = enc_vel(if b == 2 { -inner_b } else { inner_b });
        }
        // corners
        let gw = GRID_WIDTH  as usize;
        let gh = GRID_HEIGHT as usize;
        field[idx(0,    0   )] = enc_vel(0.5 * (dec_vel(field[idx(1,  0   )]) + dec_vel(field[idx(0,  1 )])));
        field[idx(gw+1, 0   )] = enc_vel(0.5 * (dec_vel(field[idx(gw, 0   )]) + dec_vel(field[idx(gw, 1 )])));
        field[idx(0,    gh+1)] = enc_vel(0.5 * (dec_vel(field[idx(1,  gh+1)]) + dec_vel(field[idx(0,  gh)])));
        field[idx(gw+1, gh+1)] = enc_vel(0.5 * (dec_vel(field[idx(gw, gh+1)]) + dec_vel(field[idx(gw, gh)])));
    }

    fn set_bnd_den(field: &mut [u16; GRID_WITH_BOUNDARY_SIZE]) {
        for y in 1..=GRID_HEIGHT as usize {
            field[idx(0,                       y)] = field[idx(1,                    y)];
            field[idx(GRID_WIDTH as usize + 1, y)] = field[idx(GRID_WIDTH as usize,  y)];
        }
        for x in 1..=GRID_WIDTH as usize {
            field[idx(x, 0                        )] = field[idx(x, 1)];
            field[idx(x, GRID_HEIGHT as usize + 1 )] = field[idx(x, GRID_HEIGHT as usize)];
        }
        let gw = GRID_WIDTH  as usize;
        let gh = GRID_HEIGHT as usize;

        // corners = average
        field[idx(0,    0   )] = ((field[idx(1,  0   )] as u32 + field[idx(0,  1  )] as u32) / 2) as u16;
        field[idx(gw+1, 0   )] = ((field[idx(gw, 0   )] as u32 + field[idx(gw, 1  )] as u32) / 2) as u16;
        field[idx(0,    gh+1)] = ((field[idx(1,  gh+1)] as u32 + field[idx(0,  gh )] as u32) / 2) as u16;
        field[idx(gw+1, gh+1)] = ((field[idx(gw, gh+1)] as u32 + field[idx(gw, gh )] as u32) / 2) as u16;
    }

    // ------------------------------------------------------------------ //
    //  diffuse (in-place Gauss-Seidel)                                   //
    // ------------------------------------------------------------------ //

    fn diffuse_vel(
        b: i32,
        field: &mut [u16; GRID_WITH_BOUNDARY_SIZE],
        diff: f32, dt: f32
    ) {
        let a = dt * diff * (GRID_WIDTH as f32).max(GRID_HEIGHT as f32);
        let inv = 1.0 / (1.0 + 4.0 * a);
        for _ in 0..SIM_STEPS {
            for y in 1..=GRID_HEIGHT as usize {
                for x in 1..=GRID_WIDTH as usize {
                    let center    = dec_vel(field[idx(x, y)]);
                    let neighbors = dec_vel(field[idx(x-1, y)])
                                  + dec_vel(field[idx(x+1, y)])
                                  + dec_vel(field[idx(x, y-1)])
                                  + dec_vel(field[idx(x, y+1)]);
                    // center is the "prev" value here (Gauss-Seidel in-place)
                    field[idx(x, y)] = enc_vel((center + a * neighbors) * inv);
                }
            }
            Self::set_bnd_vel(b, field);
        }
    }

    fn diffuse_den(
        field: &mut [u16; GRID_WITH_BOUNDARY_SIZE],
        diff: f32, dt: f32
    ) {
        let a = dt * diff * (GRID_WIDTH as f32).max(GRID_HEIGHT as f32);
        let inv = 1.0 / (1.0 + 4.0 * a);
        for _ in 0..SIM_STEPS {
            for y in 1..=GRID_HEIGHT as usize {
                for x in 1..=GRID_WIDTH as usize {
                    let center    = dec_den(field[idx(x, y)]);
                    let neighbors = dec_den(field[idx(x-1, y)])
                                  + dec_den(field[idx(x+1, y)])
                                  + dec_den(field[idx(x, y-1)])
                                  + dec_den(field[idx(x, y+1)]);
                    field[idx(x, y)] = enc_den((center + a * neighbors) * inv);
                }
            }
            Self::set_bnd_den(field);
        }
    }

    // ------------------------------------------------------------------ //
    //  advect                                                              //
    // ------------------------------------------------------------------ //

    fn advect_vel(
        b: i32,
        field: &mut [u16; GRID_WITH_BOUNDARY_SIZE],
        u: &[u16; GRID_WITH_BOUNDARY_SIZE],
        v: &[u16; GRID_WITH_BOUNDARY_SIZE],
        dt: f32,
    ) {
        let dt0_x = dt * GRID_WIDTH  as f32;
        let dt0_y = dt * GRID_HEIGHT as f32;
        let gw = GRID_WIDTH  as usize;
        let gh = GRID_HEIGHT as usize;

        let mut src = [0.0f32; GRID_WITH_BOUNDARY_SIZE];
        for i in 0..GRID_WITH_BOUNDARY_SIZE {
            src[i] = dec_vel(field[i]);
        }

        for y in 1..=gh {
            for x in 1..=gw {
                let mut px = x as f32 - dt0_x * dec_vel(u[idx(x, y)]);
                let mut py = y as f32 - dt0_y * dec_vel(v[idx(x, y)]);

                px = px.clamp(0.5, gw as f32 + 0.5);
                py = py.clamp(0.5, gh as f32 + 0.5);

                let x0 = px as usize; let x1 = x0 + 1;
                let y0 = py as usize; let y1 = y0 + 1;

                let s1 = px - x0 as f32; let s0 = 1.0 - s1;
                let t1 = py - y0 as f32; let t0 = 1.0 - t1;

                let val = s0 * (t0 * src[idx(x0, y0)] + t1 * src[idx(x0, y1)])
                        + s1 * (t0 * src[idx(x1, y0)] + t1 * src[idx(x1, y1)]);
                field[idx(x, y)] = enc_vel(val);
            }
        }
        Self::set_bnd_vel(b, field);
    }

    fn advect_den(
        field: &mut [u16; GRID_WITH_BOUNDARY_SIZE],
        u: &[u16; GRID_WITH_BOUNDARY_SIZE],
        v: &[u16; GRID_WITH_BOUNDARY_SIZE],
        dt: f32,
    ) {
        let dt0_x = dt * GRID_WIDTH  as f32;
        let dt0_y = dt * GRID_HEIGHT as f32;
        let gw = GRID_WIDTH  as usize;
        let gh = GRID_HEIGHT as usize;

        let mut src = [0.0f32; GRID_WITH_BOUNDARY_SIZE];
        for i in 0..GRID_WITH_BOUNDARY_SIZE {
            src[i] = dec_den(field[i]);
        }

        for y in 1..=gh {
            for x in 1..=gw {
                let mut px = x as f32 - dt0_x * dec_vel(u[idx(x, y)]);
                let mut py = y as f32 - dt0_y * dec_vel(v[idx(x, y)]);

                px = px.clamp(0.5, gw as f32 + 0.5);
                py = py.clamp(0.5, gh as f32 + 0.5);

                let x0 = px as usize; let x1 = x0 + 1;
                let y0 = py as usize; let y1 = y0 + 1;

                let s1 = px - x0 as f32; let s0 = 1.0 - s1;
                let t1 = py - y0 as f32; let t0 = 1.0 - t1;

                let val = s0 * (t0 * src[idx(x0, y0)] + t1 * src[idx(x0, y1)])
                        + s1 * (t0 * src[idx(x1, y0)] + t1 * src[idx(x1, y1)]);
                field[idx(x, y)] = enc_den(val);
            }
        }
        Self::set_bnd_den(field);
    }

    // ------------------------------------------------------------------ //
    //  project                                                             //
    // ------------------------------------------------------------------ //

    fn project(&mut self) {
        let gw = GRID_WIDTH  as usize;
        let gh = GRID_HEIGHT as usize;
        let h_x = 1.0 / GRID_WIDTH  as f32;
        let h_y = 1.0 / GRID_HEIGHT as f32;

        let mut div = [0.0f32; GRID_WITH_BOUNDARY_SIZE];
        let mut p   = [0.0f32; GRID_WITH_BOUNDARY_SIZE];

        // -- compute divergence --
        for y in 1..=gh {
            for x in 1..=gw {
                div[idx(x, y)] = -0.5 * (
                    h_x * (dec_vel(self.u[idx(x+1, y)]) - dec_vel(self.u[idx(x-1, y)]))
                    + h_y * (dec_vel(self.v[idx(x, y+1)]) - dec_vel(self.v[idx(x, y-1)]))
                );
            }
        }

        // boundary for divergence
        for y in 1..=gh {
            div[idx(0,    y)] = div[idx(1,  y)];
            div[idx(gw+1, y)] = div[idx(gw, y)];
        }
        for x in 1..=gw {
            div[idx(x, 0   )] = div[idx(x, 1 )];
            div[idx(x, gh+1)] = div[idx(x, gh)];
        }
        div[idx(0,    0   )] = 0.5 * (div[idx(1,  0   )] + div[idx(0,  1  )]);
        div[idx(gw+1, 0   )] = 0.5 * (div[idx(gw, 0   )] + div[idx(gw, 1  )]);
        div[idx(0,    gh+1)] = 0.5 * (div[idx(1,  gh+1)] + div[idx(0,  gh )]);
        div[idx(gw+1, gh+1)] = 0.5 * (div[idx(gw, gh+1)] + div[idx(gw, gh )]);

        // -- pressure solve (Gauss-Seidel) --
        for _ in 0..SIM_STEPS {
            for y in 1..=gh {
                for x in 1..=gw {
                    p[idx(x, y)] = (div[idx(x, y)]
                        + p[idx(x-1, y)] + p[idx(x+1, y)]
                        + p[idx(x, y-1)] + p[idx(x, y+1)]
                    ) / 4.0;
                }
            }

            // pressure boundary (b=0)
            for y in 1..=gh {
                p[idx(0,    y)] = p[idx(1,  y)];
                p[idx(gw+1, y)] = p[idx(gw, y)];
            }
            for x in 1..=gw {
                p[idx(x, 0   )] = p[idx(x, 1 )];
                p[idx(x, gh+1)] = p[idx(x, gh)];
            }

            p[idx(0,    0   )] = 0.5 * (p[idx(1,  0   )] + p[idx(0,  1  )]);
            p[idx(gw+1, 0   )] = 0.5 * (p[idx(gw, 0   )] + p[idx(gw, 1  )]);
            p[idx(0,    gh+1)] = 0.5 * (p[idx(1,  gh+1)] + p[idx(0,  gh )]);
            p[idx(gw+1, gh+1)] = 0.5 * (p[idx(gw, gh+1)] + p[idx(gw, gh )]);
        }

        // -- subtract pressure gradient --
        for y in 1..=gh {
            for x in 1..=gw {
                let u_new = dec_vel(self.u[idx(x, y)])
                    - 0.5 * (p[idx(x+1, y)] - p[idx(x-1, y)]) / h_x;
                let v_new = dec_vel(self.v[idx(x, y)])
                    - 0.5 * (p[idx(x, y+1)] - p[idx(x, y-1)]) / h_y;
                self.u[idx(x, y)] = enc_vel(u_new);
                self.v[idx(x, y)] = enc_vel(v_new);
            }
        }

        Self::set_bnd_vel(1, &mut self.u);
        Self::set_bnd_vel(2, &mut self.v);
    }

    // ------------------------------------------------------------------ //
    //  steps                                                        //
    // ------------------------------------------------------------------ //

    // fn vel_step(&mut self, visc: f32, dt: f32) {
    //     // Diffuse in-place (Gauss-Seidel, no prev needed)
    //     Self::diffuse_vel(1, &mut self.u, visc, dt);
    //     Self::diffuse_vel(2, &mut self.v, visc, dt);
    //     self.project();
    //     // Advect using a local f32 snapshot (see advect_vel comments)
    //     let u_snap = self.u;
    //     let v_snap = self.v;
    //     Self::advect_vel(1, &mut self.u, &u_snap, &v_snap, dt);
    //     Self::advect_vel(2, &mut self.v, &u_snap, &v_snap, dt);
    //     self.project();
    // }
    //
    // fn dens_step(&mut self, diff: f32, dt: f32) {
    //     Self::diffuse_den(&mut self.r, diff, dt);
    //     Self::diffuse_den(&mut self.g, diff, dt);
    //     Self::diffuse_den(&mut self.b, diff, dt);
    //
    //     // Capture velocity snapshot once for all three advect passes
    //     let u_snap = self.u;
    //     let v_snap = self.v;
    //     Self::advect_den(&mut self.r, &u_snap, &v_snap, dt);
    //     Self::advect_den(&mut self.g, &u_snap, &v_snap, dt);
    //     Self::advect_den(&mut self.b, &u_snap, &v_snap, dt);
    // }

    pub fn vel_step(&mut self, visc: f32, dt: f32) {
        let u_snap = self.u;
        let v_snap = self.v;
        Self::advect_vel(1, &mut self.u, &u_snap, &v_snap, dt);
        Self::advect_vel(2, &mut self.v, &u_snap, &v_snap, dt);
        self.project();
    }

    pub fn dens_step(&mut self, diff: f32, dt: f32) {
        let u_snap = self.u;
        let v_snap = self.v;
        Self::advect_den(&mut self.r, &u_snap, &v_snap, dt);
        Self::advect_den(&mut self.g, &u_snap, &v_snap, dt);
        Self::advect_den(&mut self.b, &u_snap, &v_snap, dt);
    }

    pub fn step(&mut self, visc: f32, diff: f32, dt: f32) {
        self.vel_step(visc, dt);
        self.dens_step(diff, dt);
    }

    // ------------------------------------------------------------------ //
    //  force injection                                                     //
    // ------------------------------------------------------------------ //

    pub fn apply_circular_force(
        &mut self,
        cx: f32,
        cy: f32,
        radius: f32,
        fx: f32,
        fy: f32,
        dt: f32,
    ) {
        let r_outer_sq = radius * radius;
        let r_inner_sq = (radius * 0.5) * (radius * 0.5);

        let x_start = (cx - radius).max(1.0) as usize;
        let x_end   = (cx + radius).min(GRID_WIDTH  as f32) as usize;
        let y_start = (cy - radius).max(1.0) as usize;
        let y_end   = (cy + radius).min(GRID_HEIGHT as f32) as usize;

        for y in y_start..=y_end {
            for x in x_start..=x_end {
                let dx = x as f32 - cx;
                let dy = y as f32 - cy;
                let dist_sq = dx * dx + dy * dy;

                if dist_sq <= r_outer_sq {
                    let i = idx(x, y);
                    let falloff = if dist_sq <= r_inner_sq {
                        1.0
                    } else {
                        (r_outer_sq - dist_sq) / (r_outer_sq - r_inner_sq)
                    };

                    let u_new = dec_vel(self.u[i]) + fx * falloff * dt;
                    let v_new = dec_vel(self.v[i]) + fy * falloff * dt;
                    self.u[i] = enc_vel(u_new);
                    self.v[i] = enc_vel(v_new);
                }
            }
        }
    }

    // ------------------------------------------------------------------ //
    //  rendering helpers                                                   //
    // ------------------------------------------------------------------ //

    /// Read a density cell as (r, g, b) as a u16
    #[inline]
    pub fn get_rgb(&self, x: usize, y: usize) -> (u16, u16, u16) {
        (
            self.r[idx(x, y)],
            self.g[idx(x, y)],
            self.b[idx(x, y)],
        )
    }
}
