use crate::nadk::display::Color565;
use crate::nadk::time::get_current_time_millis;

// Possible values: 320x240, 160x120, 80x60, 64x48, 40x30
pub const GRID_WIDTH: i32  = 40;
pub const GRID_HEIGHT: i32 = 30;
pub const GRID_WIDTH_WITH_BOUNDARY: usize  = GRID_WIDTH  as usize + 2;
pub const GRID_HEIGHT_WITH_BOUNDARY: usize = GRID_HEIGHT as usize + 2;
pub const GRID_WITH_BOUNDARY_SIZE: usize   = GRID_WIDTH_WITH_BOUNDARY
                                           * GRID_HEIGHT_WITH_BOUNDARY;

pub const CIRCLE_CX: f32       = GRID_WIDTH  as f32 / 2.0;
pub const CIRCLE_CY: f32       = GRID_HEIGHT as f32 / 2.0;
pub const CIRCLE_R_OUTER: f32  = 5.0;
pub const CIRCLE_R_INNER: f32  = 2.5;
pub const CIRCLE_OUTER_SQ: f32 = CIRCLE_R_OUTER * CIRCLE_R_OUTER;
pub const CIRCLE_INNER_SQ: f32 = CIRCLE_R_INNER * CIRCLE_R_INNER;
pub const CIRCLE_MAX_DENS: f32 = 0.05;

// Pressure solver iterations
const SIM_STEPS: usize = 4;

// Dye diffusion is skipped entirely (visually indistinguishable at DYE_DIFF=0.0001)
// Dye still spreads naturally through advection
const DYE_DECAY: f32 = 0.998;

const DIFF_ITER: usize = 4;
const VEL_VISC:  f32   = 0.0001;

// Maximum representable velocity magnitude, smaller = more precision
const VEL_MAX: f32 = 10.0;

// ------------------------------------------------------------------ //
//  index helper                                                      //
// ------------------------------------------------------------------ //

#[inline(always)]
pub const fn idx(x: usize, y: usize) -> usize {
    x + GRID_WIDTH_WITH_BOUNDARY * y
}

// ------------------------------------------------------------------ //
//  fixed-point encode / decode                                       //
// ------------------------------------------------------------------ //

/// Encode a velocity value in [-VEL_MAX, +VEL_MAX] to u16
#[inline(always)]
fn enc_vel(v: f32) -> u16 {
    let norm = (v / VEL_MAX).clamp(-1.0, 1.0);
    (norm * 32767.0 + 32768.5) as u16
}

/// Decode a u16 velocity back to f32
#[inline(always)]
fn dec_vel(raw: u16) -> f32 {
    (raw as f32 - 32768.0) / 32767.0 * VEL_MAX
}

/// Encode a dye channel value in [0, 1] to u16 (linear, full range)
#[inline(always)]
fn enc_dye(v: f32) -> u16 {
    (v.clamp(0.0, 1.0) * 65535.0 + 0.5) as u16
}

/// Decode a u16 dye value back to f32
#[inline(always)]
fn dec_dye(raw: u16) -> f32 {
    raw as f32 * (1.0 / 65535.0)
}

/// Pack an RGB triple into a Color565 directly from f32 [0,1] components
#[inline(always)]
fn rgb_to_color565(r: f32, g: f32, b: f32) -> Color565 {
    let ri = (r * 31.0 + 0.5) as u16;
    let gi = (g * 63.0 + 0.5) as u16;
    let bi = (b * 31.0 + 0.5) as u16;
    Color565::from_raw((ri << 11) | (gi << 5) | bi)
}

// ------------------------------------------------------------------ //
//  Grid                                                              //
// ------------------------------------------------------------------ //

pub struct Grid {
    // ---- live fields ------------------------------------------------
    /// Velocity X - fixed-point, signed, bias 0x8000
    pub u: [u16; GRID_WITH_BOUNDARY_SIZE],
    /// Velocity Y - fixed-point, signed, bias 0x8000
    pub v: [u16; GRID_WITH_BOUNDARY_SIZE],
    /// Dye channels - linear u16 [0, 65535]
    pub r: [u16; GRID_WITH_BOUNDARY_SIZE],
    pub g: [u16; GRID_WITH_BOUNDARY_SIZE],
    pub b: [u16; GRID_WITH_BOUNDARY_SIZE],

    // ---- persistent scratch ----
    scratch_u: [u16; GRID_WITH_BOUNDARY_SIZE],
    scratch_v: [u16; GRID_WITH_BOUNDARY_SIZE],
    scratch_r: [u16; GRID_WITH_BOUNDARY_SIZE],
    scratch_g: [u16; GRID_WITH_BOUNDARY_SIZE],
    scratch_b: [u16; GRID_WITH_BOUNDARY_SIZE],
}

// Zero velocity encodes as 0x8000
const ZERO_VEL: u16 = 0x8000;

impl Grid {
    pub fn new() -> Self {
        Self {
            u:         [ZERO_VEL; GRID_WITH_BOUNDARY_SIZE],
            v:         [ZERO_VEL; GRID_WITH_BOUNDARY_SIZE],
            r:         [0;        GRID_WITH_BOUNDARY_SIZE],
            g:         [0;        GRID_WITH_BOUNDARY_SIZE],
            b:         [0;        GRID_WITH_BOUNDARY_SIZE],
            scratch_u: [ZERO_VEL; GRID_WITH_BOUNDARY_SIZE],
            scratch_v: [ZERO_VEL; GRID_WITH_BOUNDARY_SIZE],
            scratch_r: [0;        GRID_WITH_BOUNDARY_SIZE],
            scratch_g: [0;        GRID_WITH_BOUNDARY_SIZE],
            scratch_b: [0;        GRID_WITH_BOUNDARY_SIZE],
        }
    }

    // ------------------------------------------------------------------ //
    //  boundary conditions                                               //
    // ------------------------------------------------------------------ //

    /// Apply velocity boundary to a f32 scratch array
    /// b=1 -> negate at left/right walls (u component)
    /// b=2 -> negate at top/bottom walls (v component)
    #[inline]
    fn bnd_vel_f32(b: i32, f: &mut [f32; GRID_WITH_BOUNDARY_SIZE]) {
        let gw = GRID_WIDTH  as usize;
        let gh = GRID_HEIGHT as usize;
        for y in 1..=gh {
            f[idx(0,    y)] = if b == 1 { -f[idx(1,  y)] } else { f[idx(1,  y)] };
            f[idx(gw+1, y)] = if b == 1 { -f[idx(gw, y)] } else { f[idx(gw, y)] };
        }
        for x in 1..=gw {
            f[idx(x, 0   )] = if b == 2 { -f[idx(x, 1 )] } else { f[idx(x, 1 )] };
            f[idx(x, gh+1)] = if b == 2 { -f[idx(x, gh)] } else { f[idx(x, gh)] };
        }
        f[idx(0,    0   )] = 0.5 * (f[idx(1,  0   )] + f[idx(0,  1  )]);
        f[idx(gw+1, 0   )] = 0.5 * (f[idx(gw, 0   )] + f[idx(gw, 1  )]);
        f[idx(0,    gh+1)] = 0.5 * (f[idx(1,  gh+1)] + f[idx(0,  gh )]);
        f[idx(gw+1, gh+1)] = 0.5 * (f[idx(gw, gh+1)] + f[idx(gw, gh )]);
    }

    /// Apply copy (zero-gradient) boundary to a f32 scratch array
    #[inline]
    fn bnd_copy_f32(f: &mut [f32; GRID_WITH_BOUNDARY_SIZE]) {
        let gw = GRID_WIDTH  as usize;
        let gh = GRID_HEIGHT as usize;
        for y in 1..=gh {
            f[idx(0,    y)] = f[idx(1,  y)];
            f[idx(gw+1, y)] = f[idx(gw, y)];
        }
        for x in 1..=gw {
            f[idx(x, 0   )] = f[idx(x, 1 )];
            f[idx(x, gh+1)] = f[idx(x, gh)];
        }
        f[idx(0,    0   )] = 0.5 * (f[idx(1,  0   )] + f[idx(0,  1  )]);
        f[idx(gw+1, 0   )] = 0.5 * (f[idx(gw, 0   )] + f[idx(gw, 1  )]);
        f[idx(0,    gh+1)] = 0.5 * (f[idx(1,  gh+1)] + f[idx(0,  gh )]);
        f[idx(gw+1, gh+1)] = 0.5 * (f[idx(gw, gh+1)] + f[idx(gw, gh )]);
    }

    // ------------------------------------------------------------------ //
    //  diffuse (velocity only)                                           //
    // ------------------------------------------------------------------ //

    fn diffuse_vel(
        b: i32,
        field: &mut [u16; GRID_WITH_BOUNDARY_SIZE],
        dt: f32,
    ) {
        let gw = GRID_WIDTH  as usize;
        let gh = GRID_HEIGHT as usize;
        let a   = dt * VEL_VISC * (GRID_WIDTH as f32).max(GRID_HEIGHT as f32);
        let inv = 1.0 / (1.0 + 4.0 * a);

        // Decode once into stack-local f32 scratch.
        let mut f  = [0.0f32; GRID_WITH_BOUNDARY_SIZE];
        let mut f0 = [0.0f32; GRID_WITH_BOUNDARY_SIZE];
        for i in 0..GRID_WITH_BOUNDARY_SIZE {
            let v = dec_vel(field[i]);
            f[i] = v;
            f0[i] = v;
        }

        for _ in 0..DIFF_ITER {
            for y in 1..=gh {
                for x in 1..=gw {
                    let i = idx(x, y);
                    f[i] = (f0[i] + a * (f[idx(x-1, y)] + f[idx(x+1, y)]
                                       + f[idx(x, y-1)] + f[idx(x, y+1)])) * inv;
                }
            }
            Self::bnd_vel_f32(b, &mut f);
        }

        for i in 0..GRID_WITH_BOUNDARY_SIZE {
            field[i] = enc_vel(f[i]);
        }
    }

    // ------------------------------------------------------------------ //
    //  advection pass - velocity + all three dye channels                //
    // ------------------------------------------------------------------ //

    fn advect_all(&mut self, dt: f32) {
        let gw = GRID_WIDTH  as usize;
        let gh = GRID_HEIGHT as usize;
        let dt0_x = dt * GRID_WIDTH  as f32;
        let dt0_y = dt * GRID_HEIGHT as f32;

        // Snapshot current velocity into scratch buffers (used as source).
        // We re-use scratch_u/scratch_v to avoid any extra allocation.
        self.scratch_u.copy_from_slice(&self.u);
        self.scratch_v.copy_from_slice(&self.v);
        self.scratch_r.copy_from_slice(&self.r);
        self.scratch_g.copy_from_slice(&self.g);
        self.scratch_b.copy_from_slice(&self.b);

        // Decode velocity source arrays once - hot loop reads from these.
        // Two small stack arrays (~10 KB each) that fit in L1.
        let mut uf = [0.0f32; GRID_WITH_BOUNDARY_SIZE];
        let mut vf = [0.0f32; GRID_WITH_BOUNDARY_SIZE];
        for i in 0..GRID_WITH_BOUNDARY_SIZE {
            uf[i] = dec_vel(self.scratch_u[i]);
            vf[i] = dec_vel(self.scratch_v[i]);
        }

        for y in 1..=gh {
            for x in 1..=gw {
                let i = idx(x, y);

                // ---- backtrack particle position ----
                let mut px = x as f32 - dt0_x * uf[i];
                let mut py = y as f32 - dt0_y * vf[i];
                px = px.clamp(0.5, gw as f32 + 0.5);
                py = py.clamp(0.5, gh as f32 + 0.5);

                let x0 = px as usize; let x1 = x0 + 1;
                let y0 = py as usize; let y1 = y0 + 1;
                let s1 = px - x0 as f32; let s0 = 1.0 - s1;
                let t1 = py - y0 as f32; let t0 = 1.0 - t1;

                // ---- bilinear weights - computed once for all 5 channels ----
                let w00 = s0 * t0; let w01 = s0 * t1;
                let w10 = s1 * t0; let w11 = s1 * t1;

                let i00 = idx(x0, y0); let i01 = idx(x0, y1);
                let i10 = idx(x1, y0); let i11 = idx(x1, y1);

                // ---- velocity channels (source = scratch_u/v, decoded in uf/vf) ----
                self.u[i] = enc_vel(
                    w00*uf[i00] + w01*uf[i01] + w10*uf[i10] + w11*uf[i11]
                );
                self.v[i] = enc_vel(
                    w00*vf[i00] + w01*vf[i01] + w10*vf[i10] + w11*vf[i11]
                );

                // ---- dye channels (source = scratch_r/g/b, decoded inline) ----
                let r = dec_dye(self.scratch_r[i00]) * w00
                      + dec_dye(self.scratch_r[i01]) * w01
                      + dec_dye(self.scratch_r[i10]) * w10
                      + dec_dye(self.scratch_r[i11]) * w11;
                let g = dec_dye(self.scratch_g[i00]) * w00
                      + dec_dye(self.scratch_g[i01]) * w01
                      + dec_dye(self.scratch_g[i10]) * w10
                      + dec_dye(self.scratch_g[i11]) * w11;
                let b = dec_dye(self.scratch_b[i00]) * w00
                      + dec_dye(self.scratch_b[i01]) * w01
                      + dec_dye(self.scratch_b[i10]) * w10
                      + dec_dye(self.scratch_b[i11]) * w11;

                self.r[i] = enc_dye(r * DYE_DECAY);
                self.g[i] = enc_dye(g * DYE_DECAY);
                self.b[i] = enc_dye(b * DYE_DECAY);
            }
        }

        // Boundaries for velocity
        Self::bnd_vel_u16(1, &mut self.u);
        Self::bnd_vel_u16(2, &mut self.v);

        // Boundaries for dye (copy / zero-gradient)
        Self::bnd_copy_u16(&mut self.r);
        Self::bnd_copy_u16(&mut self.g);
        Self::bnd_copy_u16(&mut self.b);
    }

    /// Apply velocity boundary directly to a u16 field (avoids decode/encode for bnd only).
    fn bnd_vel_u16(b: i32, field: &mut [u16; GRID_WITH_BOUNDARY_SIZE]) {
        let gw = GRID_WIDTH  as usize;
        let gh = GRID_HEIGHT as usize;
        for y in 1..=gh {
            let inner_l = dec_vel(field[idx(1,  y)]);
            let inner_r = dec_vel(field[idx(gw, y)]);
            field[idx(0,    y)] = enc_vel(if b == 1 { -inner_l } else { inner_l });
            field[idx(gw+1, y)] = enc_vel(if b == 1 { -inner_r } else { inner_r });
        }
        for x in 1..=gw {
            let inner_t = dec_vel(field[idx(x, 1 )]);
            let inner_b = dec_vel(field[idx(x, gh)]);
            field[idx(x, 0   )] = enc_vel(if b == 2 { -inner_t } else { inner_t });
            field[idx(x, gh+1)] = enc_vel(if b == 2 { -inner_b } else { inner_b });
        }
        field[idx(0,    0   )] = enc_vel(0.5 * (dec_vel(field[idx(1,  0   )]) + dec_vel(field[idx(0,  1  )])));
        field[idx(gw+1, 0   )] = enc_vel(0.5 * (dec_vel(field[idx(gw, 0   )]) + dec_vel(field[idx(gw, 1  )])));
        field[idx(0,    gh+1)] = enc_vel(0.5 * (dec_vel(field[idx(1,  gh+1)]) + dec_vel(field[idx(0,  gh )])));
        field[idx(gw+1, gh+1)] = enc_vel(0.5 * (dec_vel(field[idx(gw, gh+1)]) + dec_vel(field[idx(gw, gh )])));
    }

    /// Apply copy boundary directly to a u16 dye field.
    fn bnd_copy_u16(field: &mut [u16; GRID_WITH_BOUNDARY_SIZE]) {
        let gw = GRID_WIDTH  as usize;
        let gh = GRID_HEIGHT as usize;
        for y in 1..=gh {
            field[idx(0,    y)] = field[idx(1,  y)];
            field[idx(gw+1, y)] = field[idx(gw, y)];
        }
        for x in 1..=gw {
            field[idx(x, 0   )] = field[idx(x, 1 )];
            field[idx(x, gh+1)] = field[idx(x, gh)];
        }
        // corner average
        field[idx(0, 0)] = ((
            field[idx(1, 0)] as u32
            + field[idx(0, 1)] as u32
        ) / 2) as u16;
        field[idx(gw+1, 0)] = ((
            field[idx(gw, 0)] as u32
            + field[idx(gw, 1)] as u32
        ) / 2) as u16;
        field[idx(0, gh+1)] = ((
            field[idx(1, gh+1)] as u32
            + field[idx(0, gh )] as u32
        ) / 2) as u16;
        field[idx(gw+1, gh+1)] = ((
            field[idx(gw, gh+1)] as u32
            + field[idx(gw, gh)] as u32
        ) / 2) as u16;
    }

    // ------------------------------------------------------------------ //
    //  project                                                             //
    // ------------------------------------------------------------------ //

    fn project(&mut self) {
        let gw = GRID_WIDTH  as usize;
        let gh = GRID_HEIGHT as usize;
        let h_x = 1.0 / GRID_WIDTH  as f32;
        let h_y = 1.0 / GRID_HEIGHT as f32;

        // Decode velocity once into two stack-local arrays.
        // Reuse scratch_r/g/b storage is tempting but project() overlaps
        // with nothing else, so two plain stack arrays are fine and clear.
        let mut uf = [0.0f32; GRID_WITH_BOUNDARY_SIZE];
        let mut vf = [0.0f32; GRID_WITH_BOUNDARY_SIZE];
        for i in 0..GRID_WITH_BOUNDARY_SIZE {
            uf[i] = dec_vel(self.u[i]);
            vf[i] = dec_vel(self.v[i]);
        }

        let mut div = [0.0f32; GRID_WITH_BOUNDARY_SIZE];
        let mut p   = [0.0f32; GRID_WITH_BOUNDARY_SIZE];

        // -- compute divergence --
        for y in 1..=gh {
            for x in 1..=gw {
                div[idx(x, y)] = -0.5 * (
                    h_x * (uf[idx(x+1, y)] - uf[idx(x-1, y)])
                  + h_y * (vf[idx(x, y+1)] - vf[idx(x, y-1)])
                );
            }
        }
        Self::bnd_copy_f32(&mut div);

        // -- pressure solve (Gauss-Seidel, 5 iterations) --
        for _ in 0..SIM_STEPS {
            for y in 1..=gh {
                for x in 1..=gw {
                    p[idx(x, y)] = (div[idx(x, y)]
                        + p[idx(x-1, y)] + p[idx(x+1, y)]
                        + p[idx(x, y-1)] + p[idx(x, y+1)]
                    ) * 0.25;
                }
            }
            Self::bnd_copy_f32(&mut p);
        }

        // -- subtract pressure gradient (work on decoded uf/vf, encode once) --
        for y in 1..=gh {
            for x in 1..=gw {
                let i = idx(x, y);
                self.u[i] = enc_vel(uf[i] - 0.5 * (p[idx(x+1, y)] - p[idx(x-1, y)]) / h_x);
                self.v[i] = enc_vel(vf[i] - 0.5 * (p[idx(x, y+1)] - p[idx(x, y-1)]) / h_y);
            }
        }

        Self::bnd_vel_u16(1, &mut self.u);
        Self::bnd_vel_u16(2, &mut self.v);
    }

    // ------------------------------------------------------------------ //
    //  public simulation steps                                             //
    // ------------------------------------------------------------------ //

    /// Single simulation tick.
    /// Order: vel diffuse -> project -> unified advect (vel + dye) -> project
    pub fn step(&mut self, dt: f32) {
        if VEL_VISC > 0.0 {
            Self::diffuse_vel(1, &mut self.u, dt);
            Self::diffuse_vel(2, &mut self.v, dt);
            self.project();
        }
        self.advect_all(dt);
        self.project();
    }

    // ------------------------------------------------------------------ //
    //  force injection                                                   //
    // ------------------------------------------------------------------ //

    pub fn apply_circular_force(
        &mut self,
        cx: f32,
        cy: f32,
        fx: f32,
        fy: f32,
        dt: f32,
    ) {
        let r_inner_sq = CIRCLE_OUTER_SQ * 0.25;

        let x_start = (cx - CIRCLE_R_OUTER).max(1.0) as usize;
        let x_end   = (cx + CIRCLE_R_OUTER).min(GRID_WIDTH  as f32) as usize;
        let y_start = (cy - CIRCLE_R_OUTER).max(1.0) as usize;
        let y_end   = (cy + CIRCLE_R_OUTER).min(GRID_HEIGHT as f32) as usize;

        for y in y_start..=y_end {
            for x in x_start..=x_end {
                let dx = x as f32 - cx;
                let dy = y as f32 - cy;
                let dist_sq = dx * dx + dy * dy;

                if dist_sq <= CIRCLE_OUTER_SQ {
                    let i = idx(x, y);
                    let falloff = if dist_sq <= r_inner_sq {
                        1.0
                    } else {
                        (CIRCLE_OUTER_SQ - dist_sq) / (CIRCLE_OUTER_SQ - r_inner_sq)
                    };
                    self.u[i] = enc_vel(dec_vel(self.u[i]) + fx * falloff * dt);
                    self.v[i] = enc_vel(dec_vel(self.v[i]) + fy * falloff * dt);
                }
            }
        }
    }

    // ------------------------------------------------------------------ //
    //  dye injection                                                       //
    // ------------------------------------------------------------------ //

    pub fn spawn_dye(&mut self, r: f32, g: f32, b: f32) {
        for y in 1..=GRID_HEIGHT as i32 {
            for x in 1..=GRID_WIDTH as i32 {
                let dx = x as f32 - CIRCLE_CX;
                let dy = y as f32 - CIRCLE_CY;
                let dist_sq = dx * dx + dy * dy;

                if dist_sq > CIRCLE_OUTER_SQ { continue; }

                let i = idx(x as usize, y as usize);
                let falloff = if dist_sq <= CIRCLE_INNER_SQ {
                    1.0
                } else {
                    (CIRCLE_OUTER_SQ - dist_sq) / (CIRCLE_OUTER_SQ - CIRCLE_INNER_SQ)
                };
                let base = falloff * CIRCLE_MAX_DENS;

                self.r[i] = enc_dye((dec_dye(self.r[i]) + base * r).clamp(0.0, 1.0));
                self.g[i] = enc_dye((dec_dye(self.g[i]) + base * g).clamp(0.0, 1.0));
                self.b[i] = enc_dye((dec_dye(self.b[i]) + base * b).clamp(0.0, 1.0));
            }
        }
    }

    // ------------------------------------------------------------------ //
    //  rendering                                                           //
    // ------------------------------------------------------------------ //

    #[inline(always)]
    pub fn get_color(&self, i: usize) -> Color565 {
        rgb_to_color565(dec_dye(self.r[i]), dec_dye(self.g[i]), dec_dye(self.b[i]))
    }

    // ------------------------------------------------------------------ //
    //  benchmarking                                                        //
    // ------------------------------------------------------------------ //

    pub fn step_benchmarked(&mut self, dt: f32) -> (u64, u64, u64) {
        // 1. Velocity diffusion
        let t0 = get_current_time_millis();
        if VEL_VISC > 0.0 {
            Self::diffuse_vel(1, &mut self.u, dt);
            Self::diffuse_vel(2, &mut self.v, dt);
            self.project();
        }
        let t1 = get_current_time_millis();

        // 2. Advection
        self.advect_all(dt);
        let t2 = get_current_time_millis();

        // 3. Final projection
        self.project();
        let t3 = get_current_time_millis();

        // (DiffTime, AdvectTime, ProjectTime, DensTime=0)
        (t1 - t0, t2 - t1, t3 - t2)
    }
}
