use crate::{
    renderer::{utils, UiArgs},
    text::CachedGlyph,
    text_editing,
    FontAsset, LineMode, TextEditing, UiText, UiTransform,
};
use amethyst_assets::{AssetStorage, Handle};
use amethyst_core::{ecs::prelude::*, Hidden, HiddenPropagate};
use amethyst_rendy::{
    rendy::{
        command::QueueId,
        factory::{Factory, ImageState},
        hal,
        texture::{pixel::R8Unorm, TextureBuilder},
    },
    resources::Tint,
    types::Backend,
    Texture,
};
use glyph_brush::{
    ab_glyph::{Font, FontArc, PxScale, ScaleFont},
    *,
};
use std::{collections::HashMap, iter, mem, ops::Range};
use unicode_segmentation::UnicodeSegmentation;

const INITIAL_CACHE_SIZE: (u32, u32) = (512, 512);

#[derive(Default, Debug)]
pub struct UiGlyphsResource {
    glyph_texture: Option<Handle<Texture>>,
}

impl UiGlyphsResource {
    pub fn glyph_texture(&self) -> Option<&Handle<Texture>> {
        self.glyph_texture.as_ref()
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Hash)]
struct ExtraTextData {
    // Entity to which the text belongs
    entity: Entity,
    // Text color stored as linear RGBA
    color: [u32; 4],
}

impl ExtraTextData {
    fn new(entity: Entity, color: [f32; 4]) -> Self {
        Self {
            entity,
            color: [
                color[0].to_bits(),
                color[1].to_bits(),
                color[2].to_bits(),
                color[3].to_bits(),
            ],
        }
    }

    fn color(&self) -> [f32; 4] {
        [
            f32::from_bits(self.color[0]),
            f32::from_bits(self.color[1]),
            f32::from_bits(self.color[2]),
            f32::from_bits(self.color[3]),
        ]
    }
}

#[derive(Clone, Default, Debug)]
pub struct UiGlyphs {
    pub(crate) vertices: Vec<UiArgs>,
    pub(crate) selection_vertices: Vec<UiArgs>,
    pub(crate) cursor_position: (f32, f32),
    pub(crate) height: f32,
    pub(crate) space_width: f32,
}

#[derive(Copy, Clone, Debug, Hash)]
enum CustomLineBreaker {
    BuiltIn(BuiltInLineBreaker),
    None,
}

impl LineBreaker for CustomLineBreaker {
    fn line_breaks<'a>(&self, glyph_info: &'a str) -> Box<dyn Iterator<Item = LineBreak> + 'a> {
        match self {
            Self::BuiltIn(inner) => inner.line_breaks(glyph_info),
            Self::None => Box::new(None.into_iter()),
        }
    }
}

pub fn build_ui_glyphs_system<B>(
    _world: &mut World,
    _resources: &mut Resources,
) -> Box<dyn Schedulable>
where
    B: Backend,
{
    let mut glyph_brush: GlyphBrush<(Entity, UiArgs), ExtraTextData> =
        GlyphBrushBuilder::using_fonts(Vec::<FontArc>::new())
            .initial_cache_size(INITIAL_CACHE_SIZE)
            .build();

    // Maps asset handle ids to `GlyphBrush` `FontId`s
    let mut font_map = HashMap::<u32, FontId>::new();

    SystemBuilder::<()>::new("UiGlyphsSystem")
        .with_query(
            <(
                Read<UiTransform>,
                Write<UiText>,
                TryRead<Tint>,
                TryRead<TextEditing>,
            )>::query()
            .filter(!component::<Hidden>() & !component::<HiddenPropagate>()),
        )
        .with_query(Write::<UiGlyphs>::query())
        .with_query(
            <(
                Read<UiTransform>,
                Write<UiText>,
                TryRead<Tint>,
                TryWrite<TextEditing>,
                TryWrite<UiGlyphs>,
            )>::query()
            .filter(!component::<Hidden>() & !component::<HiddenPropagate>()),
        )
        .with_query(
            <(
                Read<UiTransform>,
                Write<UiText>,
                TryWrite<TextEditing>,
                TryWrite<UiGlyphs>,
            )>::query()
            .filter(!component::<Hidden>() & !component::<HiddenPropagate>()),
        )
        .read_resource::<QueueId>()
        .read_resource::<AssetStorage<FontAsset>>()
        .write_resource::<Factory<B>>()
        .write_resource::<AssetStorage<Texture>>()
        .write_resource::<UiGlyphsResource>()
        .write_component::<UiGlyphs>()
        .build(move |commands, world, resources, queries| {
            let (queue, font_storage, factory, texture_storage, glyphs_res) = resources;
            let (text_query, glyph_clear_query, glyph_draw_query, glyph_redraw_query) = queries;

            let glyph_texture_handle = glyphs_res.glyph_texture.get_or_insert_with(|| {
                let (width, height) = glyph_brush.texture_dimensions();
                texture_storage.insert(create_glyph_texture(factory, **queue, width, height))
            });

            // Unwrap won't fail because texture is created synchronously
            let mut glyph_texture = texture_storage
                .get(glyph_texture_handle)
                .and_then(B::unwrap_texture)
                .unwrap();

            for (entity, (transform, mut ui_text, tint, text_editing)) in
                text_query.iter_entities_mut(world)
            {
                ui_text.cached_glyphs.clear();

                let mut cached_glyphs = Vec::new();
                mem::swap(&mut ui_text.cached_glyphs, &mut cached_glyphs);

                let (font, font_id) = match font_storage.get(&ui_text.font) {
                    Some(font) => {
                        let font_id = *font_map
                            .entry(ui_text.font.id())
                            .or_insert_with(|| glyph_brush.add_font(font.0.clone()));

                        (font, font_id)
                    }
                    None => continue,
                };

                let tint_color = tint
                    .map(|t| utils::srgba_to_lin_rgba_array(t.0))
                    .unwrap_or([1.0, 1.0, 1.0, 1.0]);

                let base_color = utils::mul_blend_lin_rgba_arrays(
                    utils::srgba_to_lin_rgba_array(ui_text.color),
                    tint_color,
                );

                let scale = PxScale::from(ui_text.font_size);
                let scaled_font = font.0.as_scaled(scale);

                let text = match (ui_text.password, text_editing) {
                    (false, None) => vec![Text {
                        text: &ui_text.text,
                        scale,
                        font_id,
                        extra: ExtraTextData::new(entity, base_color),
                    }],
                    (false, Some(text_editing)) => {
                        let selected_color = utils::mul_blend_lin_rgba_arrays(
                            utils::srgba_to_lin_rgba_array(text_editing.selected_text_color),
                            tint_color,
                        );

                        if let Some(range) = selected_bytes(&text_editing, &ui_text.text) {
                            let start = range.start;
                            let end  = range.end;

                            vec![
                                Text {
                                    text: &ui_text.text[..start],
                                    scale,
                                    font_id,
                                    extra: ExtraTextData::new(entity, base_color),
                                },
                                Text {
                                    text: &ui_text.text[start..end],
                                    scale,
                                    font_id,
                                    extra: ExtraTextData::new(entity, selected_color),
                                },
                                Text {
                                    text: &ui_text.text[end..],
                                    scale,
                                    font_id,
                                    extra: ExtraTextData::new(entity, base_color),
                                },
                            ]
                        } else {
                            vec![Text {
                                text: &ui_text.text,
                                scale,
                                font_id,
                                extra: ExtraTextData::new(entity, base_color),
                            }]
                        }
                    }
                    (true, None) => {
                        let grapheme_count = ui_text.text.graphemes(true).count();

                        password_sections(grapheme_count)
                            .map(|text| Text {
                                text,
                                scale,
                                font_id,
                                extra: ExtraTextData::new(entity, base_color),
                            })
                            .collect()
                    }
                    (true, Some(text_editing)) => {
                        let grapheme_count = ui_text.text.graphemes(true).count();
                        let cursor_position = text_editing.cursor_position;
                        let highlight_position =
                            text_editing.cursor_position + text_editing.highlight_vector;
                        let start = cursor_position.min(highlight_position) as usize;
                        let to_end = cursor_position.max(highlight_position) as usize - start;
                        let rest = grapheme_count - to_end - start;

                        let selected_color = utils::mul_blend_lin_rgba_arrays(
                            utils::srgba_to_lin_rgba_array(text_editing.selected_text_color),
                            tint_color,
                        );

                        [
                            (start, base_color),
                            (to_end, selected_color),
                            (rest, base_color),
                        ]
                        .iter()
                        .flat_map(|&(grapheme_count, color)| {
                            password_sections(grapheme_count).map(move |text| Text {
                                text,
                                scale,
                                font_id,
                                extra: ExtraTextData::new(entity, color),
                            })
                        })
                        .collect()
                    }
                };

                let layout = match ui_text.line_mode {
                    LineMode::Single => Layout::SingleLine {
                        line_breaker: CustomLineBreaker::None,
                        h_align: ui_text.align.horizontal_align(),
                        v_align: ui_text.align.vertical_align(),
                    },
                    LineMode::Wrap => Layout::Wrap {
                        line_breaker: CustomLineBreaker::BuiltIn(
                            BuiltInLineBreaker::UnicodeLineBreaker,
                        ),
                        h_align: ui_text.align.horizontal_align(),
                        v_align: ui_text.align.vertical_align(),
                    },
                };

                let section = Section {
                    screen_position: (
                        transform.pixel_x
                            + transform.pixel_width * ui_text.align.normalized_offset().0,
                        -(transform.pixel_y
                            + transform.pixel_height * ui_text.align.normalized_offset().1),
                    ),
                    bounds: (transform.pixel_width, transform.pixel_height),
                    layout: Layout::default(),
                    text,
                };

                let mut visible_glyphs_iter = glyph_brush.glyphs_custom_layout(&section, &layout);

                if ui_text.password {
                    let all_glyphs_iter = visible_glyphs_iter.map(|section_glyph| CachedGlyph {
                        x: section_glyph.glyph.position.x,
                        y: -section_glyph.glyph.position.y,
                        advance_width: scaled_font.h_advance(section_glyph.glyph.id),
                    });

                    cached_glyphs.extend(all_glyphs_iter);
                } else {
                    let mut last_section_glyph = visible_glyphs_iter.next();
                    let mut last_cached_glyph = Option::<CachedGlyph>::None;

                    let all_glyphs_iter = ui_text.text.chars().map(|c| {
                        let (x, y) = match last_cached_glyph {
                            Some(last_cached_glyph) => (
                                last_cached_glyph.x + last_cached_glyph.advance_width,
                                last_cached_glyph.y,
                            ),
                            None => (0.0, 0.0),
                        };

                        let cached_glyph = match last_section_glyph {
                            Some(section_glyph) => {
                                if scaled_font.glyph_id(c) == section_glyph.glyph.id {
                                    let cached_glyph = CachedGlyph {
                                        x: section_glyph.glyph.position.x,
                                        y: -section_glyph.glyph.position.y,
                                        advance_width: scaled_font
                                            .h_advance(section_glyph.glyph.id),
                                    };

                                    last_section_glyph = visible_glyphs_iter.next();
                                    cached_glyph
                                } else {
                                    CachedGlyph {
                                        x,
                                        y,
                                        advance_width: scaled_font
                                            .h_advance(scaled_font.glyph_id(c)),
                                    }
                                }
                            }
                            None => CachedGlyph {
                                x,
                                y,
                                advance_width: scaled_font.h_advance(scaled_font.glyph_id(c)),
                            },
                        };

                        last_cached_glyph = Some(cached_glyph);
                        cached_glyph
                    });

                    cached_glyphs.extend(all_glyphs_iter);
                }

                glyph_brush.queue_custom_layout(section, &layout);
                mem::swap(&mut ui_text.cached_glyphs, &mut cached_glyphs);
            }

            loop {
                let action = glyph_brush.process_queued(
                    |rect, data| unsafe {
                        factory
                            .upload_image(
                                glyph_texture.image().clone(),
                                rect.width(),
                                rect.height(),
                                hal::image::SubresourceLayers {
                                    aspects: hal::format::Aspects::COLOR,
                                    level: 0,
                                    layers: 0..1,
                                },
                                hal::image::Offset {
                                    x: rect.min[0] as _,
                                    y: rect.min[1] as _,
                                    z: 0,
                                },
                                hal::image::Extent {
                                    width: rect.width(),
                                    height: rect.height(),
                                    depth: 1,
                                },
                                data,
                                ImageState {
                                    queue: **queue,
                                    stage: hal::pso::PipelineStage::FRAGMENT_SHADER,
                                    access: hal::image::Access::SHADER_READ,
                                    layout: hal::image::Layout::General,
                                },
                                ImageState {
                                    queue: **queue,
                                    stage: hal::pso::PipelineStage::FRAGMENT_SHADER,
                                    access: hal::image::Access::SHADER_READ,
                                    layout: hal::image::Layout::General,
                                },
                            )
                            .unwrap();
                    },
                    move |glyph| {
                        let bounds_min_x = glyph.bounds.min.x as f32;
                        let bounds_min_y = glyph.bounds.min.y as f32;
                        let bounds_max_x = glyph.bounds.max.x as f32;
                        let bounds_max_y = glyph.bounds.max.y as f32;

                        let mut uv = glyph.tex_coords;
                        let mut coords_min_x = glyph.pixel_coords.min.x as f32;
                        let mut coords_min_y = glyph.pixel_coords.min.y as f32;
                        let mut coords_max_x = glyph.pixel_coords.max.x as f32;
                        let mut coords_max_y = glyph.pixel_coords.max.y as f32;

                        if coords_max_x > bounds_max_x {
                            let old_width = coords_max_x - coords_min_x;
                            coords_max_x = bounds_max_x;
                            uv.max.x = uv.min.x
                                + (uv.max.x - uv.min.x) * (coords_max_x - coords_min_x) / old_width;
                        }
                        if coords_min_x < bounds_min_x {
                            let old_width = coords_max_x - coords_min_x;
                            coords_min_x = bounds_min_x;
                            uv.min.x = uv.max.x
                                - (uv.max.x - uv.min.x) * (coords_max_x - coords_min_x) / old_width;
                        }
                        if coords_max_y > bounds_max_y {
                            let old_height = coords_max_y - coords_min_y;
                            coords_max_y = bounds_max_y;
                            uv.max.y = uv.min.y
                                + (uv.max.y - uv.min.y) * (coords_max_y - coords_min_y)
                                    / old_height;
                        }
                        if coords_min_y < bounds_min_y {
                            let old_height = coords_max_y - coords_min_y;
                            coords_min_y = bounds_min_y;
                            uv.min.y = uv.max.y
                                - (uv.max.y - uv.min.y) * (coords_max_y - coords_min_y)
                                    / old_height;
                        }

                        let position = [
                            (coords_max_x + coords_min_x) / 2.0,
                            -(coords_max_y + coords_min_y) / 2.0,
                        ];
                        let dimensions =
                            [(coords_max_x - coords_min_x), (coords_max_y - coords_min_y)];
                        let tex_coords_bounds = [uv.min.x, uv.min.y, uv.max.x, uv.max.y];

                        (
                            glyph.extra.entity,
                            UiArgs {
                                position: position.into(),
                                dimensions: dimensions.into(),
                                tex_coords_bounds: tex_coords_bounds.into(),
                                color: glyph.extra.color().into(),
                                color_bias: [1.0, 1.0, 1.0, 0.0].into(),
                            },
                        )
                    },
                );

                match action {
                    Ok(BrushAction::Draw(vertices)) => {
                        let mut current_glyph = 0;

                        for mut glyphs in glyph_clear_query.iter_mut(world) {
                            glyphs.selection_vertices.clear();
                            glyphs.vertices.clear();
                        }

                        for (entity, (transform, ui_text, tint, text_editing, mut glyphs)) in
                            glyph_draw_query.iter_entities_mut(world)
                        {
                            let vertices = vertices[current_glyph..]
                                .iter()
                                .take_while(|(e, _)| *e == entity)
                                .map(|(_, v)| {
                                    current_glyph += 1;
                                    *v
                                });

                            if let Some(glyphs) = glyphs.as_mut() {
                                glyphs.vertices.extend(vertices);
                            } else {
                                commands.add_component(
                                    entity,
                                    UiGlyphs {
                                        vertices: vertices.collect(),
                                        ..UiGlyphs::default()
                                    },
                                );
                            }

                            if let Some(text_editing) = text_editing {
                                let font = font_storage
                                    .get(&ui_text.font)
                                    .expect("Font with rendered glyphs must be loaded");
                                let scale = PxScale::from(ui_text.font_size);
                                let scaled_font = font.0.as_scaled(scale);

                                let height = scaled_font.ascent() - scaled_font.descent();
                                let offset = (scaled_font.ascent() + scaled_font.descent()) / 2.0;

                                let highlight_range =
                                    highlighted_glyphs_range(&text_editing, &ui_text);

                                let color = if let Some(tint) = tint {
                                    utils::mul_blend_srgba_to_lin_rgba_array(
                                        &text_editing.selected_background_color,
                                        &tint.0,
                                    )
                                } else {
                                    utils::srgba_to_lin_rgba_array(
                                        text_editing.selected_background_color,
                                    )
                                };

                                let selection_ui_args_iter = ui_text.cached_glyphs[highlight_range]
                                    .iter()
                                    .map(|g| UiArgs {
                                        position: [g.x + g.advance_width / 2.0, g.y + offset]
                                            .into(),
                                        dimensions: [g.advance_width, height].into(),
                                        tex_coords_bounds: [0.0, 0.0, 1.0, 1.0].into(),
                                        color: color.into(),
                                        color_bias: [0.0, 0.0, 0.0, 0.0].into(),
                                    });

                                if let Some(mut glyphs) = glyphs {
                                    glyphs.selection_vertices.extend(selection_ui_args_iter);
                                    glyphs.height = height;
                                    glyphs.space_width =
                                        scaled_font.h_advance(scaled_font.glyph_id(' '));

                                    update_cursor_position(
                                        &mut glyphs,
                                        &ui_text,
                                        &transform,
                                        text_editing.cursor_position as usize,
                                        offset,
                                    );
                                }
                            }
                        }

                        break;
                    }
                    Ok(BrushAction::ReDraw) => {
                        for (transform, ui_text, text_editing, glyphs) in
                            glyph_redraw_query.iter_mut(world)
                        {
                            if let (Some(text_editing), Some(mut glyphs)) = (text_editing, glyphs) {
                                let font = font_storage
                                    .get(&ui_text.font)
                                    .expect("Font with rendered glyphs must be loaded");
                                let scale = PxScale::from(ui_text.font_size);
                                let scaled_font = font.0.as_scaled(scale);

                                let height = scaled_font.ascent() - scaled_font.descent();
                                let offset = (scaled_font.ascent() + scaled_font.descent()) / 2.0;

                                glyphs.height = height;
                                glyphs.space_width =
                                    scaled_font.h_advance(scaled_font.glyph_id(' '));

                                update_cursor_position(
                                    &mut glyphs,
                                    &ui_text,
                                    &transform,
                                    text_editing.cursor_position as usize,
                                    offset,
                                );
                            }
                        }

                        break;
                    }
                    Err(BrushError::TextureTooSmall {
                        suggested: (width, height),
                    }) => {
                        texture_storage.replace(
                            glyph_texture_handle,
                            create_glyph_texture(factory, **queue, width, height),
                        );

                        glyph_texture = texture_storage
                            .get(glyph_texture_handle)
                            .and_then(B::unwrap_texture)
                            .unwrap();

                        glyph_brush.resize_texture(width, height);
                    }
                }
            }
        })
}

fn create_glyph_texture<B>(
    factory: &mut Factory<B>,
    queue: QueueId,
    width: u32,
    height: u32,
) -> Texture
where
    B: Backend,
{
    use hal::format::{Component as C, Swizzle};

    log::trace!(
        "Creating new glyph texture with size ({}, {})",
        width,
        height
    );

    TextureBuilder::new()
        .with_kind(hal::image::Kind::D2(width, height, 1, 1))
        .with_view_kind(hal::image::ViewKind::D2)
        .with_data_width(width)
        .with_data_height(height)
        .with_data(vec![R8Unorm { repr: [0] }; (width * height) as _])
        // This swizzle is required when working with `R8Unorm` on Metal.
        // Glyph texture is biased towards 1.0 using the "color_bias" attribute.
        .with_swizzle(Swizzle(C::Zero, C::Zero, C::Zero, C::R))
        .build(
            ImageState {
                queue,
                stage: hal::pso::PipelineStage::FRAGMENT_SHADER,
                access: hal::image::Access::SHADER_READ,
                layout: hal::image::Layout::General,
            },
            factory,
        )
        .map(B::wrap_texture)
        .expect("Failed to create glyph texture")
}

fn update_cursor_position(
    glyph_data: &mut UiGlyphs,
    ui_text: &UiText,
    transform: &UiTransform,
    cursor_position: usize,
    offset: f32,
) {
    glyph_data.cursor_position = if let Some(glyph) = ui_text.cached_glyphs.get(cursor_position) {
        (glyph.x, glyph.y + offset)
    } else if let Some(glyph) = ui_text.cached_glyphs.last() {
        (glyph.x + glyph.advance_width, glyph.y + offset)
    } else {
        (
            transform.pixel_x + transform.pixel_width * ui_text.align.normalized_offset().0,
            transform.pixel_y + transform.pixel_height * ui_text.align.normalized_offset().1,
        )
    }
}

fn selected_bytes(text_editing: &TextEditing, text: &str) -> Option<Range<usize>> {
    if text_editing.highlight_vector == 0 {
        return None;
    }

    let start = text_editing.cursor_position.min(
        text_editing.cursor_position + text_editing.highlight_vector,
    ) as usize;

    let to_end = text_editing.cursor_position.max(
        text_editing.cursor_position + text_editing.highlight_vector,
    ) as usize - start - 1;

    let mut indexes = text.grapheme_indices(true).map(|(i, _)| i);
    let start_byte = indexes.nth(start).unwrap_or(text.len());
    let end_byte = indexes.nth(to_end).unwrap_or(text.len());

    if start_byte == end_byte {
        None
    } else {
        Some(start_byte..end_byte)
    }
}

fn highlighted_glyphs_range(text_editing: &TextEditing, ui_text: &UiText) -> Range<usize> {
    let cursor_position = text_editing.cursor_position as usize;
    let highlight_position =
        (text_editing.cursor_position + text_editing.highlight_vector) as usize;
    let glyph_count = ui_text.cached_glyphs.len();

    let start = cursor_position.min(highlight_position).min(glyph_count);
    let end = cursor_position.max(highlight_position).min(glyph_count);

    start..end
}

fn password_sections(grapheme_count: usize) -> impl Iterator<Item = &'static str> {
    const PASSWORD_STR: &'static str = "••••••••••••••••";
    const PASSWORD_STR_GRAPHEME_COUNT: usize = 16;
    const PASSWORD_CHAR_GRAPHEME_BYTE_COUNT: usize = 3;

    let full_chunks = grapheme_count / PASSWORD_STR_GRAPHEME_COUNT;
    let remaining_graphemes = grapheme_count % PASSWORD_STR_GRAPHEME_COUNT;

    iter::repeat(PASSWORD_STR).take(full_chunks).chain(Some(
        &PASSWORD_STR[0..remaining_graphemes * PASSWORD_CHAR_GRAPHEME_BYTE_COUNT],
    ))
}
