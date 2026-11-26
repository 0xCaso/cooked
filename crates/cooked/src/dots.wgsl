// The soft dots (dots.rs): one quad over a slot, each pixel shaded by the 3 x 3 dots around it. A dot's radius and
// alpha grow with its light, with a halo when lit and a white core when bright (Hex's Dot scope, rune_hud.wgsl).

struct Field {
    // the slot in the pass's target, device px: x, y, w, h
    rect: vec4f,
    // target width, height; 1 when the target holds linear values; antialiasing width, px
    canvas: vec4f,
    // columns, rows, then between dot centres, px: x, y
    grid: vec4f,
    // the first dot's centre in the slot, px; the size the radius follows, px; an unlit dot's alpha
    first: vec4f,
    base: vec4f,
    red: vec4f,
}

@group(0) @binding(0) var<uniform> u: Field;
// One u32 a dot, row-major from the top: the light as a normalized u16, bit 16 red. Four to a vec4u.
@group(0) @binding(1) var<uniform> dots: array<vec4u, 512>;

@vertex
fn vs(@builtin(vertex_index) v: u32) -> @builtin(position) vec4f {
    // the slot, grown by a dot so the edge dots' halos are not cut
    let grow = u.first.z;
    let corners = array<vec2f, 6>(vec2f(0.0, 0.0), vec2f(1.0, 0.0), vec2f(0.0, 1.0), vec2f(0.0, 1.0), vec2f(1.0, 0.0), vec2f(1.0, 1.0));
    let p = u.rect.xy - vec2f(grow) + corners[v] * (u.rect.zw + vec2f(2.0 * grow));
    return vec4f(p.x / u.canvas.x * 2.0 - 1.0, 1.0 - p.y / u.canvas.y * 2.0, 0.0, 1.0);
}

fn coverage(distance: f32, aa: f32) -> f32 {
    return clamp(0.5 - distance / aa, 0.0, 1.0);
}

fn over(destination: ptr<function, vec4f>, color: vec3f, alpha_in: f32) {
    let alpha = clamp(alpha_in, 0.0, 1.0);
    (*destination) = vec4f(color * alpha + (*destination).rgb * (1.0 - alpha), alpha + (*destination).a * (1.0 - alpha));
}

fn dot_at(index: i32) -> vec2f {
    let word = dots[index / 4][index % 4];
    return vec2f(f32(word & 0xffffu) / 65535.0, f32((word >> 16u) & 1u));
}

@fragment
fn fs(@builtin(position) position: vec4f) -> @location(0) vec4f {
    let local = position.xy - u.rect.xy;
    let cols = i32(u.grid.x + 0.5);
    let rows = i32(u.grid.y + 0.5);
    let step = u.grid.zw;
    let size = u.first.z;
    let off = u.first.w;
    let aa = u.canvas.w;
    let cell = (local - u.first.xy) / step;
    let i0 = i32(round(cell.x));
    let j0 = i32(round(cell.y));
    var result = vec4f(0.0);
    for (var dj = -1; dj <= 1; dj++) {
        for (var di = -1; di <= 1; di++) {
            let i = i0 + di;
            let j = j0 + dj;
            if (i < 0 || j < 0 || i >= cols || j >= rows) {
                continue;
            }
            let value = dot_at(j * cols + i);
            let p = clamp(value.x, 0.0, 1.0);
            let color = select(u.base.rgb, u.red.rgb, value.y > 0.5);
            let d = length(local - (u.first.xy + vec2f(f32(i), f32(j)) * step));
            let radius = (0.25 + 0.17 * pow(p, 0.6)) * size;
            if (p > 0.05) {
                over(&result, color, 0.14 * p * p * coverage(d - radius * 2.1, aa));
            }
            if (p > 0.02) {
                over(&result, color, (off + (1.0 - off) * pow(p, 1.25)) * coverage(d - radius, aa));
            } else {
                over(&result, vec3f(1.0), off * coverage(d - radius, aa));
            }
            if (p > 0.4) {
                over(&result, vec3f(1.0), 0.5 * (p - 0.4) * coverage(d - radius * 0.55, aa));
            }
        }
    }
    if (u.canvas.z < 0.5 || result.a <= 0.0) {
        return result;
    }
    // the target holds linear values: decode the display colour
    let c = result.rgb / result.a;
    let linear = select(pow((c + 0.055) / 1.055, vec3f(2.4)), c / 12.92, c <= vec3f(0.04045));
    return vec4f(linear * result.a, result.a);
}
