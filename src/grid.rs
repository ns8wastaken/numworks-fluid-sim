// Possible values: 320x240, 160x120, 80x60, 64x48
pub const GRID_WIDTH: i32                  = 80;
pub const GRID_HEIGHT: i32                 = 60;
pub const GRID_WIDTH_WITH_BOUNDARY: usize  = GRID_WIDTH as usize + 2;
pub const GRID_HEIGHT_WITH_BOUNDARY: usize = GRID_HEIGHT as usize + 2;
pub const GRID_WITH_BOUNDARY_SIZE: usize   = GRID_WIDTH_WITH_BOUNDARY * GRID_HEIGHT_WITH_BOUNDARY;

macro_rules! idx {
    ($x:expr, $y:expr) => {
        $x as usize + GRID_WIDTH_WITH_BOUNDARY * $y as usize
    };
}

pub const fn idx(x: usize, y: usize) -> usize {
    x + GRID_WIDTH_WITH_BOUNDARY * y
}

/* The x's are the boundary, the O's are what will be visible
 * xxxxx
 * xOOOx
 * xOOOx
 * xOOOx
 * xxxxx
 */

pub struct Grid {
    pub u:      [f32; GRID_WITH_BOUNDARY_SIZE],
    pub v:      [f32; GRID_WITH_BOUNDARY_SIZE],
    pub u_prev: [f32; GRID_WITH_BOUNDARY_SIZE],
    pub v_prev: [f32; GRID_WITH_BOUNDARY_SIZE],

    // Dye Channels
    pub r:       [f32; GRID_WITH_BOUNDARY_SIZE],
    pub g:       [f32; GRID_WITH_BOUNDARY_SIZE],
    pub b:       [f32; GRID_WITH_BOUNDARY_SIZE],
    pub r_prev:  [f32; GRID_WITH_BOUNDARY_SIZE],
    pub g_prev:  [f32; GRID_WITH_BOUNDARY_SIZE],
    pub b_prev:  [f32; GRID_WITH_BOUNDARY_SIZE],
}

impl Grid {
    pub fn new() -> Self {
        Self {
            u:      [0.0; GRID_WITH_BOUNDARY_SIZE],
            v:      [0.0; GRID_WITH_BOUNDARY_SIZE],
            u_prev: [0.0; GRID_WITH_BOUNDARY_SIZE],
            v_prev: [0.0; GRID_WITH_BOUNDARY_SIZE],
            r:      [0.0; GRID_WITH_BOUNDARY_SIZE],
            g:      [0.0; GRID_WITH_BOUNDARY_SIZE],
            b:      [0.0; GRID_WITH_BOUNDARY_SIZE],
            r_prev: [0.0; GRID_WITH_BOUNDARY_SIZE],
            g_prev: [0.0; GRID_WITH_BOUNDARY_SIZE],
            b_prev: [0.0; GRID_WITH_BOUNDARY_SIZE],
        }
    }

    // ------------------------------------------------------------------ //
    //  boundary                                                            //
    // ------------------------------------------------------------------ //

    fn set_bnd(b: i32, field: &mut [f32; GRID_WITH_BOUNDARY_SIZE]) {
        // left and right walls
        for y in 1..=GRID_HEIGHT {
            field[idx!(0,              y)] = if b == 1 { -field[idx!(1,          y)] } else { field[idx!(1,          y)] };
            field[idx!(GRID_WIDTH + 1, y)] = if b == 1 { -field[idx!(GRID_WIDTH, y)] } else { field[idx!(GRID_WIDTH, y)] };
        }
        // top and bottom walls
        for x in 1..=GRID_WIDTH {
            field[idx!(x,  0              )] = if b == 2 { -field[idx!(x,  1          )] } else { field[idx!(x,  1)] };
            field[idx!(x,  GRID_HEIGHT + 1)] = if b == 2 { -field[idx!(x,  GRID_HEIGHT)] } else { field[idx!(x,  GRID_HEIGHT)] };
        }
        // corners — average of the two neighbours
        field[idx!(0,              0            )] = 0.5 * (field[idx!(1,          0            )] + field[idx!(0,          1)]);
        field[idx!(GRID_WIDTH + 1, 0            )] = 0.5 * (field[idx!(GRID_WIDTH, 0            )] + field[idx!(GRID_WIDTH, 1)]);
        field[idx!(0,              GRID_HEIGHT+1)] = 0.5 * (field[idx!(1,          GRID_HEIGHT+1)] + field[idx!(0,          GRID_HEIGHT)]);
        field[idx!(GRID_WIDTH + 1, GRID_HEIGHT+1)] = 0.5 * (field[idx!(GRID_WIDTH, GRID_HEIGHT+1)] + field[idx!(GRID_WIDTH, GRID_HEIGHT)]);
    }

    // ------------------------------------------------------------------ //
    //  sources                                                             //
    // ------------------------------------------------------------------ //

    fn add_source_field(
        field: &mut [f32; GRID_WITH_BOUNDARY_SIZE],
        src: &[f32; GRID_WITH_BOUNDARY_SIZE],
        dt: f32
    ) {
        for i in 0..GRID_WITH_BOUNDARY_SIZE {
            field[i] += dt * src[i];
        }
    }

    fn add_source_uv(&mut self, dt: f32) {
        let u_prev = self.u_prev;
        let v_prev = self.v_prev;
        Self::add_source_field(&mut self.u, &u_prev, dt);
        Self::add_source_field(&mut self.v, &v_prev, dt);
    }

    // ------------------------------------------------------------------ //
    //  swaps                                                               //
    // ------------------------------------------------------------------ //

    fn swap_density(&mut self) {
        core::mem::swap(&mut self.r, &mut self.r_prev);
        core::mem::swap(&mut self.g, &mut self.g_prev);
        core::mem::swap(&mut self.b, &mut self.b_prev);
    }

    fn swap_u(&mut self) {
        core::mem::swap(&mut self.u, &mut self.u_prev);
    }

    fn swap_v(&mut self) {
        core::mem::swap(&mut self.v, &mut self.v_prev);
    }

    // ------------------------------------------------------------------ //
    //  diffuse                                                             //
    // ------------------------------------------------------------------ //

    fn diffuse_field(
        b: i32,
        field: &mut [f32; GRID_WITH_BOUNDARY_SIZE],
        field_prev: &[f32; GRID_WITH_BOUNDARY_SIZE],
        diff: f32,
        dt: f32
    ) {
        let a = dt * diff * (GRID_WIDTH as f32).max(GRID_HEIGHT as f32);
        for _ in 0..20 {
            for y in 1..=GRID_HEIGHT {
                for x in 1..=GRID_WIDTH {
                    let neighbors = field[idx!(x-1, y)]
                                  + field[idx!(x+1, y)]
                                  + field[idx!(x, y-1)]
                                  + field[idx!(x, y+1)];
                    field[idx!(x, y)] = (
                        field_prev[idx!(x, y)]
                        + a * neighbors
                    ) / (1.0 + 4.0 * a);
                }
            }
            Self::set_bnd(b, field);
        }
    }

    fn diffuse_u(&mut self, visc: f32, dt: f32) {
        let u_prev = self.u_prev;
        Self::diffuse_field(1, &mut self.u, &u_prev, visc, dt);
    }

    fn diffuse_v(&mut self, visc: f32, dt: f32) {
        let v_prev = self.v_prev;
        Self::diffuse_field(2, &mut self.v, &v_prev, visc, dt);
    }

    // ------------------------------------------------------------------ //
    //  advect                                                              //
    // ------------------------------------------------------------------ //

    fn advect_field(
        b: i32,
        field: &mut [f32; GRID_WITH_BOUNDARY_SIZE],
        field_prev: &[f32; GRID_WITH_BOUNDARY_SIZE],
        u: &[f32; GRID_WITH_BOUNDARY_SIZE],
        v: &[f32; GRID_WITH_BOUNDARY_SIZE],
        dt: f32,
    ) {
        let dt0_x = dt * GRID_WIDTH as f32;
        let dt0_y = dt * GRID_HEIGHT as f32;

        for y in 1..=GRID_HEIGHT {
            for x in 1..=GRID_WIDTH {
                let mut px = x as f32 - dt0_x * u[idx!(x, y)];
                let mut py = y as f32 - dt0_y * v[idx!(x, y)];

                px = px.clamp(0.5, GRID_WIDTH as f32 + 0.5);
                py = py.clamp(0.5, GRID_HEIGHT as f32 + 0.5);

                let x0 = px as i32; let x1 = x0 + 1;
                let y0 = py as i32; let y1 = y0 + 1;

                let s1 = px - x0 as f32; let s0 = 1.0 - s1;
                let t1 = py - y0 as f32; let t0 = 1.0 - t1;

                field[idx!(x, y)] =
                    s0 * (t0 * field_prev[idx!(x0, y0)]
                        + t1 * field_prev[idx!(x0, y1)])
                  + s1 * (t0 * field_prev[idx!(x1, y0)]
                        + t1 * field_prev[idx!(x1, y1)]);
            }
        }

        Self::set_bnd(b, field);
    }

    fn advect_u(&mut self, dt: f32) {
        let u_prev = self.u_prev;
        let v_prev = self.v_prev;
        Self::advect_field(1, &mut self.u, &u_prev, &u_prev, &v_prev, dt);
    }

    fn advect_v(&mut self, dt: f32) {
        let u_prev = self.u_prev;
        let v_prev = self.v_prev;
        Self::advect_field(2, &mut self.v, &v_prev, &u_prev, &v_prev, dt);
    }

    // ------------------------------------------------------------------ //
    //  project                                                             //
    // ------------------------------------------------------------------ //

    fn project(&mut self) {
        let h_x = 1.0 / GRID_WIDTH as f32;
        let h_y = 1.0 / GRID_HEIGHT as f32;

        // compute divergence into u_prev, zero pressure into v_prev
        for y in 1..=GRID_HEIGHT {
            for x in 1..=GRID_WIDTH {
                self.u_prev[idx!(x, y)] = -0.5 * (
                    h_x * (self.u[idx!(x+1, y)] - self.u[idx!(x-1, y)]) +
                    h_y * (self.v[idx!(x, y+1)] - self.v[idx!(x, y-1)])
                );
                self.v_prev[idx!(x, y)] = 0.0;
            }
        }
        Self::set_bnd(0, &mut self.u_prev);
        Self::set_bnd(0, &mut self.v_prev);

        // Gauss-Seidel pressure solve
        for _ in 0..20 {
            for y in 1..=GRID_HEIGHT {
                for x in 1..=GRID_WIDTH {
                    self.v_prev[idx!(x, y)] = (
                          self.u_prev[idx!(x,   y  )]
                        + self.v_prev[idx!(x-1, y  )]
                        + self.v_prev[idx!(x+1, y  )]
                        + self.v_prev[idx!(x,   y-1)]
                        + self.v_prev[idx!(x,   y+1)]
                    ) / 4.0;
                }
            }
            Self::set_bnd(0, &mut self.v_prev);
        }

        // subtract pressure gradient
        for y in 1..=GRID_HEIGHT {
            for x in 1..=GRID_WIDTH {
                self.u[idx!(x, y)] -= 0.5 * (self.v_prev[idx!(x+1, y  )] - self.v_prev[idx!(x-1, y  )]) / h_x;
                self.v[idx!(x, y)] -= 0.5 * (self.v_prev[idx!(x,   y+1)] - self.v_prev[idx!(x,   y-1)]) / h_y;
            }
        }
        Self::set_bnd(1, &mut self.u);
        Self::set_bnd(2, &mut self.v);
    }

    // ------------------------------------------------------------------ //
    //  steps                                                               //
    // ------------------------------------------------------------------ //

    pub fn dens_step(&mut self, diff: f32, dt: f32) {
        // self.add_source(dt);
        // self.swap_density();
        // self.diffuse(diff, dt);
        // self.swap_density();
        // self.advect(dt);
        // 1. Add sources to all channels
        Self::add_source_field(&mut self.r, &self.r_prev, dt);
        Self::add_source_field(&mut self.g, &self.g_prev, dt);
        Self::add_source_field(&mut self.b, &self.b_prev, dt);

        self.swap_density();
        Self::diffuse_field(0, &mut self.r, &self.r_prev, diff, dt);
        Self::diffuse_field(0, &mut self.g, &self.g_prev, diff, dt);
        Self::diffuse_field(0, &mut self.b, &self.b_prev, diff, dt);

        self.swap_density();
        Self::advect_field(0, &mut self.r, &self.r_prev, &self.u, &self.v, dt);
        Self::advect_field(0, &mut self.g, &self.g_prev, &self.u, &self.v, dt);
        Self::advect_field(0, &mut self.b, &self.b_prev, &self.u, &self.v, dt);
    }

    pub fn vel_step(&mut self, visc: f32, dt: f32) {
        self.add_source_uv(dt);
        self.swap_u();
        self.diffuse_u(visc, dt);
        self.swap_v();
        self.diffuse_v(visc, dt);
        self.project();
        self.swap_u();
        self.swap_v();
        self.advect_u(dt);
        self.advect_v(dt);
        self.project();
    }

    pub fn step(&mut self, visc: f32, diff: f32, dt: f32) {
        self.vel_step(visc, dt);
        self.dens_step(diff, dt);
    }

    // ------------------------------------------------------------------ //
    //  force injection (call before step())                               //
    // ------------------------------------------------------------------ //

    pub fn apply_circular_source(
        &mut self,
        cx: f32,
        cy: f32,
        radius: f32,
        fx: f32,
        fy: f32,
        dt: f32
    ) {
        let r_outer_sq = radius * radius;
        let r_inner_sq = (radius * 0.5) * (radius * 0.5); // Core is half the radius
        let max_density = 0.025;

        // Iterate over a bounding box around the circle to save cycles
        let x_start = (cx - radius).max(1.0) as usize;
        let x_end = (cx + radius).min(GRID_WIDTH as f32) as usize;
        let y_start = (cy - radius).max(1.0) as usize;
        let y_end = (cy + radius).min(GRID_HEIGHT as f32) as usize;

        for y in y_start..=y_end {
            for x in x_start..=x_end {
                let dx = x as f32 - cx;
                let dy = y as f32 - cy;
                let dist_sq = dx * dx + dy * dy;

                if dist_sq <= r_outer_sq {
                    let i = idx!(x, y);

                    let falloff = if dist_sq <= r_inner_sq {
                        1.0
                    } else {
                        (r_outer_sq - dist_sq) / (r_outer_sq - r_inner_sq)
                    };

                    self.r_prev[i] = max_density * falloff;
                    self.g_prev[i] = max_density * falloff;
                    self.b_prev[i] = max_density * falloff;

                    self.u[i] += fx * falloff * dt;
                    self.v[i] += fy * falloff * dt;
                }
            }
        }
    }

    pub fn clear_sources(&mut self) {
        self.u_prev = [0.0; GRID_WITH_BOUNDARY_SIZE];
        self.v_prev = [0.0; GRID_WITH_BOUNDARY_SIZE];
        self.r_prev = [0.0; GRID_WITH_BOUNDARY_SIZE];
        self.g_prev = [0.0; GRID_WITH_BOUNDARY_SIZE];
        self.b_prev = [0.0; GRID_WITH_BOUNDARY_SIZE];
    }
}
