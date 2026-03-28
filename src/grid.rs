use crate::nadk::display::Color565;

// Possible values: 320x240, 160x120, 80x60, 64x48, 40x30
pub const GRID_WIDTH:  i32 = 40;
pub const GRID_HEIGHT: i32 = 30;
pub const GRID_WIDTH_WITH_BOUNDARY:  usize = GRID_WIDTH  as usize + 2;
pub const GRID_HEIGHT_WITH_BOUNDARY: usize = GRID_HEIGHT as usize + 2;
pub const GRID_WITH_BOUNDARY_SIZE:   usize = GRID_WIDTH_WITH_BOUNDARY
                                           * GRID_HEIGHT_WITH_BOUNDARY;

pub const CIRCLE_CX: f32       = GRID_WIDTH as f32 / 2.0;
pub const CIRCLE_CY: f32       = GRID_HEIGHT as f32 / 2.0;
pub const CIRCLE_R_OUTER: f32  = 5.0;
pub const CIRCLE_R_INNER: f32  = 2.5; // fade start
pub const CIRCLE_OUTER_SQ: f32 = CIRCLE_R_OUTER * CIRCLE_R_OUTER;
pub const CIRCLE_INNER_SQ: f32 = CIRCLE_R_INNER * CIRCLE_R_INNER;
pub const CIRCLE_MAX_DENS: f32 = 0.05;

const SIM_STEPS: usize = 10;
const DYE_DECAY: f32 = 0.999;

const DIFF_ITER: usize = 4;   // iterations for both diffuse solves
const DYE_DIFF: f32    = 0.0001;
const VEL_VISC: f32    = 0.0001; // near-zero for low viscosity

const AVG_MASK: u32 = !(1 << 9 | 1 << 19 | 1 << 29); // 0xDFEFFBFF

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

#[inline(always)]
fn enc_rgb(r: f32, g: f32, b: f32) -> u32 {
    let r10 = (r.clamp(0.0, 1.0) * 1023.0 + 0.5) as u32;
    let g10 = (g.clamp(0.0, 1.0) * 1023.0 + 0.5) as u32;
    let b10 = (b.clamp(0.0, 1.0) * 1023.0 + 0.5) as u32;
    r10 | (g10 << 10) | (b10 << 20)
}

#[inline(always)]
fn dec_r(rgb: u32) -> f32 { (rgb & 0x3FF) as f32 / 1023.0 }

#[inline(always)]
fn dec_g(rgb: u32) -> f32 { ((rgb >> 10) & 0x3FF) as f32 / 1023.0 }

#[inline(always)]
fn dec_b(rgb: u32) -> f32 { ((rgb >> 20) & 0x3FF) as f32 / 1023.0 }

// ------------------------------------------------------------------ //
//  Grid                                                                //
// ------------------------------------------------------------------ //

pub struct Grid {
    /// Velocity X — fixed-point, signed, bias 0x8000
    pub u: [u16; GRID_WITH_BOUNDARY_SIZE],
    /// Velocity Y — fixed-point, signed, bias 0x8000
    pub v: [u16; GRID_WITH_BOUNDARY_SIZE],

    /// Dye channels — fixed-point, unsigned
    pub rgb: [u32; GRID_WITH_BOUNDARY_SIZE],
}

// Zero velocity encodes as 0x8000 (midpoint), zero density as 0x0000.
const ZERO_VEL: u16 = 0x8000;

impl Grid {
    pub fn new() -> Self {
        Self {
            u: [ZERO_VEL; GRID_WITH_BOUNDARY_SIZE],
            v: [ZERO_VEL; GRID_WITH_BOUNDARY_SIZE],
            rgb: [0; GRID_WITH_BOUNDARY_SIZE],
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

    fn set_bnd_rgb(field: &mut [u32; GRID_WITH_BOUNDARY_SIZE]) {
        let gw = GRID_WIDTH  as usize;
        let gh = GRID_HEIGHT as usize;
        // left and right walls — density copies neighbour (b=0)
        for y in 1..=gh {
            field[idx(0,    y)] = field[idx(1,  y)];
            field[idx(gw+1, y)] = field[idx(gw, y)];
        }
        // top and bottom walls
        for x in 1..=gw {
            field[idx(x, 0   )] = field[idx(x, 1 )];
            field[idx(x, gh+1)] = field[idx(x, gh)];
        }
        // corners — integer average per packed channel
        // safe because channels are in [0, 1023] so sum fits in u32
        let avg = |a: u32, b: u32| -> u32 {
            ((a >> 1) & AVG_MASK) + ((b >> 1) & AVG_MASK)
        };
        field[idx(0,    0   )] = avg(field[idx(1,  0   )], field[idx(0,  1  )]);
        field[idx(gw+1, 0   )] = avg(field[idx(gw, 0   )], field[idx(gw, 1  )]);
        field[idx(0,    gh+1)] = avg(field[idx(1,  gh+1)], field[idx(0,  gh )]);
        field[idx(gw+1, gh+1)] = avg(field[idx(gw, gh+1)], field[idx(gw, gh )]);
    }

    // ------------------------------------------------------------------ //
    //  diffuse                                                           //
    // ------------------------------------------------------------------ //

    fn diffuse_rgb(rgb: &mut [u32; GRID_WITH_BOUNDARY_SIZE], dt: f32) {
        let gw = GRID_WIDTH  as usize;
        let gh = GRID_HEIGHT as usize;
        let a   = dt * DYE_DIFF * (GRID_WIDTH as f32).max(GRID_HEIGHT as f32);
        let inv = 1.0 / (1.0 + 4.0 * a);

        // decode
        let mut rf = [0.0f32; GRID_WITH_BOUNDARY_SIZE];
        let mut gf = [0.0f32; GRID_WITH_BOUNDARY_SIZE];
        let mut bf = [0.0f32; GRID_WITH_BOUNDARY_SIZE];
        // keep a snapshot of the original values as the "prev" source
        let mut rf0 = [0.0f32; GRID_WITH_BOUNDARY_SIZE];
        let mut gf0 = [0.0f32; GRID_WITH_BOUNDARY_SIZE];
        let mut bf0 = [0.0f32; GRID_WITH_BOUNDARY_SIZE];
        for i in 0..GRID_WITH_BOUNDARY_SIZE {
            let r = dec_r(rgb[i]);
            let g = dec_g(rgb[i]);
            let b = dec_b(rgb[i]);
            rf[i] = r; rf0[i] = r;
            gf[i] = g; gf0[i] = g;
            bf[i] = b; bf0[i] = b;
        }

        for _ in 0..DIFF_ITER {
            for y in 1..=gh {
                for x in 1..=gw {
                    let i   = idx(x, y);
                    let i_l = idx(x-1, y); let i_r = idx(x+1, y);
                    let i_u = idx(x, y-1); let i_d = idx(x, y+1);
                    rf[i] = (rf0[i] + a * (rf[i_l] + rf[i_r] + rf[i_u] + rf[i_d])) * inv;
                    gf[i] = (gf0[i] + a * (gf[i_l] + gf[i_r] + gf[i_u] + gf[i_d])) * inv;
                    bf[i] = (bf0[i] + a * (bf[i_l] + bf[i_r] + bf[i_u] + bf[i_d])) * inv;
                }
            }
            // boundary on f32 scratch directly
            for y in 1..=gh {
                rf[idx(0,    y)] = rf[idx(1,  y)];
                gf[idx(0,    y)] = gf[idx(1,  y)];
                bf[idx(0,    y)] = bf[idx(1,  y)];
                rf[idx(gw+1, y)] = rf[idx(gw, y)];
                gf[idx(gw+1, y)] = gf[idx(gw, y)];
                bf[idx(gw+1, y)] = bf[idx(gw, y)];
            }
            for x in 1..=gw {
                rf[idx(x, 0)]    = rf[idx(x, 1)];
                gf[idx(x, 0)]    = gf[idx(x, 1)];
                bf[idx(x, 0)]    = bf[idx(x, 1)];
                rf[idx(x, gh+1)] = rf[idx(x, gh)];
                gf[idx(x, gh+1)] = gf[idx(x, gh)];
                bf[idx(x, gh+1)] = bf[idx(x, gh)];
            }
            rf[idx(0,    0)]    = 0.5 * (rf[idx(1,  0)]    + rf[idx(0,  1)]);
            gf[idx(0,    0)]    = 0.5 * (gf[idx(1,  0)]    + gf[idx(0,  1)]);
            bf[idx(0,    0)]    = 0.5 * (bf[idx(1,  0)]    + bf[idx(0,  1)]);
            rf[idx(gw+1, 0)]    = 0.5 * (rf[idx(gw, 0)]    + rf[idx(gw, 1)]);
            gf[idx(gw+1, 0)]    = 0.5 * (gf[idx(gw, 0)]    + gf[idx(gw, 1)]);
            bf[idx(gw+1, 0)]    = 0.5 * (bf[idx(gw, 0)]    + bf[idx(gw, 1)]);
            rf[idx(0,    gh+1)] = 0.5 * (rf[idx(1,  gh+1)] + rf[idx(0,  gh)]);
            gf[idx(0,    gh+1)] = 0.5 * (gf[idx(1,  gh+1)] + gf[idx(0,  gh)]);
            bf[idx(0,    gh+1)] = 0.5 * (bf[idx(1,  gh+1)] + bf[idx(0,  gh)]);
            rf[idx(gw+1, gh+1)] = 0.5 * (rf[idx(gw, gh+1)] + rf[idx(gw, gh)]);
            gf[idx(gw+1, gh+1)] = 0.5 * (gf[idx(gw, gh+1)] + gf[idx(gw, gh)]);
            bf[idx(gw+1, gh+1)] = 0.5 * (bf[idx(gw, gh+1)] + bf[idx(gw, gh)]);
        }

        // encode
        for i in 0..GRID_WITH_BOUNDARY_SIZE {
            rgb[i] = enc_rgb(rf[i], gf[i], bf[i]);
        }
    }

    fn diffuse_vel(
        b: i32,
        field: &mut [u16; GRID_WITH_BOUNDARY_SIZE],
        dt: f32,
    ) {
        let gw = GRID_WIDTH  as usize;
        let gh = GRID_HEIGHT as usize;
        let a   = dt * VEL_VISC * (GRID_WIDTH as f32).max(GRID_HEIGHT as f32);
        let inv = 1.0 / (1.0 + 4.0 * a);

        // decode + snapshot
        let mut f  = [0.0f32; GRID_WITH_BOUNDARY_SIZE];
        let mut f0 = [0.0f32; GRID_WITH_BOUNDARY_SIZE];
        for i in 0..GRID_WITH_BOUNDARY_SIZE {
            let v = dec_vel(field[i]);
            f[i] = v; f0[i] = v;
        }

        for _ in 0..DIFF_ITER {
            for y in 1..=gh {
                for x in 1..=gw {
                    let i = idx(x, y);
                    f[i] = (f0[i] + a * (f[idx(x-1,y)] + f[idx(x+1,y)]
                                       + f[idx(x,y-1)] + f[idx(x,y+1)])) * inv;
                }
            }
            // inline boundary on f32 scratch
            for y in 1..=gh {
                f[idx(0,    y)] = if b == 1 { -f[idx(1,  y)] } else { f[idx(1,  y)] };
                f[idx(gw+1, y)] = if b == 1 { -f[idx(gw, y)] } else { f[idx(gw, y)] };
            }
            for x in 1..=gw {
                f[idx(x, 0   )] = if b == 2 { -f[idx(x, 1 )] } else { f[idx(x, 1 )] };
                f[idx(x, gh+1)] = if b == 2 { -f[idx(x, gh)] } else { f[idx(x, gh)] };
            }
            f[idx(0,    0   )] = 0.5*(f[idx(1,  0   )]+f[idx(0,  1  )]);
            f[idx(gw+1, 0   )] = 0.5*(f[idx(gw, 0   )]+f[idx(gw, 1  )]);
            f[idx(0,    gh+1)] = 0.5*(f[idx(1,  gh+1)]+f[idx(0,  gh )]);
            f[idx(gw+1, gh+1)] = 0.5*(f[idx(gw, gh+1)]+f[idx(gw, gh )]);
        }

        // encode
        for i in 0..GRID_WITH_BOUNDARY_SIZE { field[i] = enc_vel(f[i]); }
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
        let mut uf  = [0.0f32; GRID_WITH_BOUNDARY_SIZE];
        let mut vf  = [0.0f32; GRID_WITH_BOUNDARY_SIZE];
        for i in 0..GRID_WITH_BOUNDARY_SIZE {
            src[i] = dec_vel(field[i]);
            uf[i]  = dec_vel(u[i]);
            vf[i]  = dec_vel(v[i]);
        }

        for y in 1..=gh {
            for x in 1..=gw {
                let i = idx(x, y);
                let mut px = x as f32 - dt0_x * uf[i];
                let mut py = y as f32 - dt0_y * vf[i];
                px = px.clamp(0.5, gw as f32 + 0.5);
                py = py.clamp(0.5, gh as f32 + 0.5);
                let x0 = px as usize; let x1 = x0 + 1;
                let y0 = py as usize; let y1 = y0 + 1;
                let s1 = px - x0 as f32; let s0 = 1.0 - s1;
                let t1 = py - y0 as f32; let t0 = 1.0 - t1;
                field[idx(x, y)] = enc_vel(
                    s0 * (t0 * src[idx(x0, y0)] + t1 * src[idx(x0, y1)])
                  + s1 * (t0 * src[idx(x1, y0)] + t1 * src[idx(x1, y1)])
                );
            }
        }
        Self::set_bnd_vel(b, field);
    }

    fn advect_rgb(
        rgb: &mut [u32; GRID_WITH_BOUNDARY_SIZE],
        u:   &[u16; GRID_WITH_BOUNDARY_SIZE],
        v:   &[u16; GRID_WITH_BOUNDARY_SIZE],
        dt:  f32,
    ) {
        let dt0_x = dt * GRID_WIDTH  as f32;
        let dt0_y = dt * GRID_HEIGHT as f32;
        let gw = GRID_WIDTH  as usize;
        let gh = GRID_HEIGHT as usize;

        // decode everything once before the hot loop
        let mut uf = [0.0f32; GRID_WITH_BOUNDARY_SIZE];
        let mut vf = [0.0f32; GRID_WITH_BOUNDARY_SIZE];
        let mut rf = [0.0f32; GRID_WITH_BOUNDARY_SIZE];
        let mut gf = [0.0f32; GRID_WITH_BOUNDARY_SIZE];
        let mut bf = [0.0f32; GRID_WITH_BOUNDARY_SIZE];
        for i in 0..GRID_WITH_BOUNDARY_SIZE {
            uf[i] = dec_vel(u[i]);
            vf[i] = dec_vel(v[i]);
            rf[i] = dec_r(rgb[i]);
            gf[i] = dec_g(rgb[i]);
            bf[i] = dec_b(rgb[i]);
        }

        for y in 1..=gh {
            for x in 1..=gw {
                let i = idx(x, y);

                let mut px = x as f32 - dt0_x * uf[i];
                let mut py = y as f32 - dt0_y * vf[i];
                px = px.clamp(0.5, gw as f32 + 0.5);
                py = py.clamp(0.5, gh as f32 + 0.5);

                let x0 = px as usize; let x1 = x0 + 1;
                let y0 = py as usize; let y1 = y0 + 1;
                let s1 = px - x0 as f32; let s0 = 1.0 - s1;
                let t1 = py - y0 as f32; let t0 = 1.0 - t1;

                // compute bilinear weights once, apply to all 3 channels
                let w00 = s0 * t0; let w01 = s0 * t1;
                let w10 = s1 * t0; let w11 = s1 * t1;

                let i00 = idx(x0, y0); let i01 = idx(x0, y1);
                let i10 = idx(x1, y0); let i11 = idx(x1, y1);

                let r = (w00*rf[i00] + w01*rf[i01] + w10*rf[i10] + w11*rf[i11]) * DYE_DECAY;
                let g = (w00*gf[i00] + w01*gf[i01] + w10*gf[i10] + w11*gf[i11]) * DYE_DECAY;
                let b = (w00*bf[i00] + w01*bf[i01] + w10*bf[i10] + w11*bf[i11]) * DYE_DECAY;

                rgb[i] = enc_rgb(r, g, b);
            }
        }

        Self::set_bnd_rgb(rgb);
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
        // for y in 1..=gh {
        //     for x in 1..=gw {
        //         let u_new = dec_vel(self.u[idx(x, y)])
        //             - 0.5 * (p[idx(x+1, y)] - p[idx(x-1, y)]) / h_x;
        //         let v_new = dec_vel(self.v[idx(x, y)])
        //             - 0.5 * (p[idx(x, y+1)] - p[idx(x, y-1)]) / h_y;
        //         self.u[idx(x, y)] = enc_vel(u_new);
        //         self.v[idx(x, y)] = enc_vel(v_new);
        //     }
        // }
        let mut uf = [0.0f32; GRID_WITH_BOUNDARY_SIZE];
        let mut vf = [0.0f32; GRID_WITH_BOUNDARY_SIZE];
        for i in 0..GRID_WITH_BOUNDARY_SIZE {
            uf[i] = dec_vel(self.u[i]);
            vf[i] = dec_vel(self.v[i]);
        }
        for y in 1..=gh {
            for x in 1..=gw {
                let i = idx(x, y);
                self.u[i] = enc_vel(uf[i] - 0.5 * (p[idx(x+1, y)] - p[idx(x-1, y)]) / h_x);
                self.v[i] = enc_vel(vf[i] - 0.5 * (p[idx(x, y+1)] - p[idx(x, y-1)]) / h_y);
            }
        }

        Self::set_bnd_vel(1, &mut self.u);
        Self::set_bnd_vel(2, &mut self.v);
    }

    // ------------------------------------------------------------------ //
    //  steps                                                        //
    // ------------------------------------------------------------------ //

    pub fn vel_step(&mut self, dt: f32) {
        // Skip vel diffuse entirely if viscosity is zero — saves the stack alloc
        if VEL_VISC > 0.0 {
            Self::diffuse_vel(1, &mut self.u, dt);
            Self::diffuse_vel(2, &mut self.v, dt);
            self.project();
        }
        let u_snap = self.u;
        let v_snap = self.v;
        Self::advect_vel(1, &mut self.u, &u_snap, &v_snap, dt);
        Self::advect_vel(2, &mut self.v, &u_snap, &v_snap, dt);
        self.project();
    }

    pub fn dens_step(&mut self, dt: f32) {
        Self::diffuse_rgb(&mut self.rgb, dt);
        let u_snap = self.u;
        let v_snap = self.v;
        Self::advect_rgb(&mut self.rgb, &u_snap, &v_snap, dt);
    }

    pub fn step(&mut self, dt: f32) {
        self.vel_step(dt);
        self.dens_step(dt);
    }

    // ------------------------------------------------------------------ //
    //  force injection                                                     //
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

    pub fn spawn_dye(&mut self, r: f32, g: f32, b: f32) {
        for y in 1..=GRID_HEIGHT as i32 {
            for x in 1..=GRID_WIDTH as i32 {
                let dx = x as f32 - CIRCLE_CX;
                let dy = y as f32 - CIRCLE_CY;
                let dist_sq = dx * dx + dy * dy;

                if dist_sq <= CIRCLE_OUTER_SQ {
                    let i = idx(x as usize, y as usize);

                    let falloff = if dist_sq <= CIRCLE_INNER_SQ {
                        1.0
                    } else {
                        (CIRCLE_OUTER_SQ - dist_sq) / (CIRCLE_OUTER_SQ - CIRCLE_INNER_SQ)
                    };

                    let base = falloff * CIRCLE_MAX_DENS; // [0, 1]

                    // Decode current packed value
                    let cur = self.rgb[i];
                    let cr = dec_r(cur);
                    let cg = dec_g(cur);
                    let cb = dec_b(cur);

                    // Saturating add per channel
                    let nr = (cr + base * r).clamp(0.0, 1.0);
                    let ng = (cg + base * g).clamp(0.0, 1.0);
                    let nb = (cb + base * b).clamp(0.0, 1.0);

                    self.rgb[i] = enc_rgb(nr, ng, nb);
                }
            }
        }
    }

    #[inline(always)]
    pub fn get_color(&self, idx: usize) -> Color565 {
        let rgb = self.rgb[idx];
        let r = ((rgb & 0x3FF) >> 2) as u16;
        let g = (((rgb >> 10) & 0x3FF) >> 2) as u16;
        let b = (((rgb >> 20) & 0x3FF) >> 2) as u16;
        Color565::from_rgb888(r, g, b)
    }
}
