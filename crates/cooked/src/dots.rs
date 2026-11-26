//! The soft dots on the GPU: the chart's and the island wave's fields (`cooked_model::soft_dots`), drawn in their
//! effect slot by `dots.wgsl`, one quad a field.

use cooked_model::soft_dots::Dot;
use rune::render::EffectSite;

/// The most dots one field holds: the shader's dot array.
pub const MAX_DOTS: usize = 2048;
/// Antialiasing width, device px.
const AA: f32 = 1.0;

/// Where a field's dots sit in its slot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Layout {
    /// One dot in the middle of each cell, the cells filling the slot (the chart).
    Fill,
    /// Dots this far apart (points), the field centred in the slot (the braid).
    Pitch(f32),
}

/// One field to draw: its dots, row-major from the top, and how it looks.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub cols: usize,
    pub rows: usize,
    pub layout: Layout,
    /// An unlit dot's alpha.
    pub off: f32,
    pub dots: Vec<Dot>,
}

/// `dots.wgsl`'s `Field`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Uniforms {
    rect: [f32; 4],
    canvas: [f32; 4],
    grid: [f32; 4],
    first: [f32; 4],
    base: [f32; 4],
    red: [f32; 4],
}

const UNIFORM_BYTES: u64 = std::mem::size_of::<Uniforms>() as u64;
const DOT_BYTES: u64 = (MAX_DOTS * 4) as u64;

impl Uniforms {
    /// The field in `site`'s slot. Colours are display (sRGB) values.
    fn new(field: &Field, site: &EffectSite<'_>, base: [f32; 3], red: [f32; 3]) -> Self {
        let r = site.instance.rect;
        let [x, y, w, h] = [r[0] - site.origin[0], r[1] - site.origin[1], r[2], r[3]];
        let (cols, rows) = (field.cols.max(1) as f32, field.rows.max(1) as f32);
        let (step, first, size) = match field.layout {
            Layout::Fill => {
                let (cw, ch) = (w / cols, h / rows);
                ([cw, ch], [cw / 2.0, ch / 2.0], cw.min(ch))
            }
            Layout::Pitch(pt) => {
                let p = pt * site.scale;
                let first = [(w - (cols - 1.0) * p) / 2.0, (h - (rows - 1.0) * p) / 2.0];
                ([p, p], first, p)
            }
        };
        Self {
            rect: [x, y, w, h],
            canvas: [
                site.size[0] as f32,
                site.size[1] as f32,
                f32::from(u8::from(!site.srgb_blend)),
                AA,
            ],
            grid: [cols, rows, step[0], step[1]],
            first: [first[0], first[1], size, field.off],
            base: [base[0], base[1], base[2], 1.0],
            red: [red[0], red[1], red[2], 1.0],
        }
    }

    fn bytes(&self) -> Vec<u8> {
        [
            self.rect,
            self.canvas,
            self.grid,
            self.first,
            self.base,
            self.red,
        ]
        .iter()
        .flatten()
        .flat_map(|v| v.to_ne_bytes())
        .collect()
    }
}

/// A field's dots as the shader reads them: the light as a u16, bit 16 red; padded to `MAX_DOTS`.
fn packed(dots: &[Dot]) -> Vec<u8> {
    let mut out = vec![0u8; DOT_BYTES as usize];
    for (i, d) in dots.iter().take(MAX_DOTS).enumerate() {
        let word = (d.power.clamp(0.0, 1.0) * 65535.0).round() as u32 | (u32::from(d.red) << 16);
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_ne_bytes());
    }
    out
}

/// One field's buffers, by its slot's mark.
struct Site {
    mark: u32,
    uniforms: wgpu::Buffer,
    dots: wgpu::Buffer,
    bind: wgpu::BindGroup,
    /// What the buffers hold, so an unchanged field writes nothing.
    written: (Uniforms, Vec<u8>),
}

/// The dot pipelines (one per target format) and each field's buffers, on one device.
pub struct DotPainter {
    module: wgpu::ShaderModule,
    layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    pipelines: Vec<(wgpu::TextureFormat, wgpu::RenderPipeline)>,
    sites: Vec<Site>,
}

impl DotPainter {
    pub fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cooked dots"),
            source: wgpu::ShaderSource::Wgsl(include_str!("dots.wgsl").into()),
        });
        let entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cooked dots"),
            entries: &[entry(0), entry(1)],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cooked dots"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        Self {
            module,
            layout,
            pipeline_layout,
            pipelines: Vec::new(),
            sites: Vec::new(),
        }
    }

    fn pipeline(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat) -> usize {
        if let Some(i) = self.pipelines.iter().position(|(f, _)| *f == format) {
            return i;
        }
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("cooked dots"),
            layout: Some(&self.pipeline_layout),
            vertex: wgpu::VertexState {
                module: &self.module,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &self.module,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        self.pipelines.push((format, pipeline));
        self.pipelines.len() - 1
    }

    fn site(&mut self, device: &wgpu::Device, mark: u32) -> usize {
        if let Some(i) = self.sites.iter().position(|s| s.mark == mark) {
            return i;
        }
        let buffer = |label, size| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let uniforms = buffer("cooked dots field", UNIFORM_BYTES);
        let dots = buffer("cooked dots", DOT_BYTES);
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cooked dots"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: dots.as_entire_binding(),
                },
            ],
        });
        self.sites.push(Site {
            mark,
            uniforms,
            dots,
            bind,
            written: (Uniforms::default(), Vec::new()),
        });
        self.sites.len() - 1
    }

    /// Draws `field` in `site`'s slot. The buffer writes land ahead of this frame's commands: each field has its own,
    /// so the one draw that reads them sees this frame's values.
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pass: &mut wgpu::RenderPass<'_>,
        site: &EffectSite<'_>,
        field: &Field,
        colors: ([f32; 3], [f32; 3]),
    ) {
        if field.cols * field.rows == 0 || field.cols * field.rows > MAX_DOTS {
            return;
        }
        let p = self.pipeline(device, site.format);
        let s = self.site(device, site.mark);
        let uniforms = Uniforms::new(field, site, colors.0, colors.1);
        let dots = packed(&field.dots);
        let slot = &mut self.sites[s];
        if slot.written.0 != uniforms {
            queue.write_buffer(&slot.uniforms, 0, &uniforms.bytes());
        }
        if slot.written.1 != dots {
            queue.write_buffer(&slot.dots, 0, &dots);
        }
        slot.written = (uniforms, dots);
        pass.set_pipeline(&self.pipelines[p].1);
        pass.set_bind_group(0, &slot.bind, &[]);
        pass.draw(0..6, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dots_pack_light_and_red() {
        let bytes = packed(&[
            Dot {
                power: 1.0,
                red: true,
            },
            Dot {
                power: 0.5,
                red: false,
            },
        ]);
        assert_eq!(bytes.len(), DOT_BYTES as usize);
        let word = |i: usize| u32::from_ne_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap());
        assert_eq!(word(0), 0xffff | 1 << 16);
        assert_eq!(word(1), 32768);
        assert_eq!(word(2), 0);
    }

    #[test]
    fn uniforms_match_the_shader_struct() {
        assert_eq!(UNIFORM_BYTES, 6 * 16);
        assert_eq!(Uniforms::default().bytes().len(), UNIFORM_BYTES as usize);
    }
}
